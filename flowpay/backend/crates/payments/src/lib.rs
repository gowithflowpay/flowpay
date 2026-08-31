use flowpay_domain::{AtomicAmount, MerchantId, OverpaymentPolicy, PaymentId, PaymentState};
use serde::{Deserialize, Serialize};
use sha3::{Digest, Keccak256};
use thiserror::Error;

const SALT_DOMAIN: &[u8] = b"FLOWPAY_EVM_CHECKOUT_V1";

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ObservedDeposit {
    pub chain: String,
    pub tx_hash: String,
    pub asset_symbol: String,
    pub token_contract: Option<String>,
    pub amount: AtomicAmount,
    pub final_enough: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ReconciliationInput {
    pub expected_chain: String,
    pub expected_asset_symbol: String,
    pub expected_token_contract: Option<String>,
    pub expected_amount: AtomicAmount,
    pub current_state: PaymentState,
    pub overpayment_policy: OverpaymentPolicy,
    pub deposits: Vec<ObservedDeposit>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ReconciliationResult {
    pub next_state: PaymentState,
    pub confirmed_expected_total: AtomicAmount,
    pub wrong_asset_detected: bool,
    pub reason_code: String,
}

#[derive(Debug, Error)]
pub enum ReconciliationError {
    #[error("payment is already terminal")]
    Terminal,
    #[error("illegal state transition from {from:?} to {to:?}")]
    IllegalTransition {
        from: PaymentState,
        to: PaymentState,
    },
}

#[must_use]
pub fn derive_checkout_salt(merchant_id: MerchantId, payment_id: PaymentId) -> [u8; 32] {
    let domain = Keccak256::digest(SALT_DOMAIN);
    let mut hasher = Keccak256::new();
    // Equivalent to a fixed-width tuple encoding for three 32-byte fields.
    hasher.update(domain);
    hasher.update([0_u8; 16]);
    hasher.update(merchant_id.0.as_bytes());
    hasher.update([0_u8; 16]);
    hasher.update(payment_id.0.as_bytes());
    hasher.finalize().into()
}

pub fn reconcile(input: &ReconciliationInput) -> Result<ReconciliationResult, ReconciliationError> {
    if input.current_state.is_terminal() {
        return Err(ReconciliationError::Terminal);
    }

    let mut total = AtomicAmount::zero();
    let mut wrong_asset = false;
    let mut any_expected = false;
    let mut all_expected_final = true;

    for deposit in &input.deposits {
        let chain_matches = deposit.chain.eq_ignore_ascii_case(&input.expected_chain);
        let symbol_matches = deposit
            .asset_symbol
            .eq_ignore_ascii_case(&input.expected_asset_symbol);
        let contract_matches = match (&input.expected_token_contract, &deposit.token_contract) {
            (None, None) => true,
            (Some(a), Some(b)) => a.eq_ignore_ascii_case(b),
            _ => false,
        };
        if chain_matches && symbol_matches && contract_matches {
            any_expected = true;
            if deposit.final_enough {
                total = total.checked_add(&deposit.amount);
            } else {
                all_expected_final = false;
            }
        } else {
            // A token transfer on another chain is an exception even when the token
            // contract/symbol happens to match the expected asset.
            wrong_asset = true;
        }
    }

    let (next, reason) = if wrong_asset && !any_expected {
        (PaymentState::WrongAsset, "wrong_asset_detected")
    } else if wrong_asset {
        // Mixed expected/unexpected assets are never a successful payment. Keep the
        // expected deposit visible, but require an explicit claim for the exception.
        (PaymentState::ClaimPending, "mixed_assets_requires_review")
    } else if any_expected && !all_expected_final {
        (PaymentState::Confirming, "awaiting_confirmations")
    } else if total < input.expected_amount {
        if total.is_zero() {
            (PaymentState::Waiting, "no_final_expected_deposit")
        } else {
            (PaymentState::PartiallyPaid, "underpaid")
        }
    } else if total == input.expected_amount {
        (PaymentState::Confirmed, "exact_amount_confirmed")
    } else {
        match input.overpayment_policy {
            OverpaymentPolicy::AcceptAndRecord => (PaymentState::Overpaid, "overpayment_accepted"),
            OverpaymentPolicy::RequireReview | OverpaymentPolicy::RejectSettlement => {
                (PaymentState::ClaimPending, "overpayment_requires_review")
            }
        }
    };

    if next != input.current_state && !input.current_state.can_transition_to(next) {
        // Monitoring commonly re-evaluates from WAITING after a final snapshot. Model the
        // detection edge explicitly instead of allowing the reconciler to skip it silently.
        let allowed_snapshot_jump = matches!(
            (input.current_state, next),
            (PaymentState::Waiting, PaymentState::Confirming)
                | (PaymentState::Waiting, PaymentState::WrongAsset)
                | (PaymentState::Waiting, PaymentState::PartiallyPaid)
                | (PaymentState::Waiting, PaymentState::Confirmed)
                | (PaymentState::Waiting, PaymentState::Overpaid)
                | (PaymentState::Detected, PaymentState::PartiallyPaid)
                | (PaymentState::Detected, PaymentState::Confirmed)
                | (PaymentState::Detected, PaymentState::Overpaid)
        );
        if !allowed_snapshot_jump {
            return Err(ReconciliationError::IllegalTransition {
                from: input.current_state,
                to: next,
            });
        }
    }

    Ok(ReconciliationResult {
        next_state: next,
        confirmed_expected_total: total,
        wrong_asset_detected: wrong_asset,
        reason_code: reason.to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn a(v: &str) -> AtomicAmount {
        AtomicAmount::from_str(v).unwrap()
    }
    fn dep(amount: &str, final_enough: bool) -> ObservedDeposit {
        ObservedDeposit {
            chain: "base".into(),
            tx_hash: "0x1".into(),
            asset_symbol: "USDC".into(),
            token_contract: Some("0xusdc".into()),
            amount: a(amount),
            final_enough,
        }
    }
    fn input(deposits: Vec<ObservedDeposit>) -> ReconciliationInput {
        ReconciliationInput {
            expected_chain: "base".into(),
            expected_asset_symbol: "USDC".into(),
            expected_token_contract: Some("0xusdc".into()),
            expected_amount: a("100"),
            current_state: PaymentState::Waiting,
            overpayment_policy: OverpaymentPolicy::RequireReview,
            deposits,
        }
    }

    #[test]
    fn exact_payment_confirms() {
        assert_eq!(
            reconcile(&input(vec![dep("100", true)]))
                .unwrap()
                .next_state,
            PaymentState::Confirmed
        );
    }
    #[test]
    fn partials_aggregate() {
        assert_eq!(
            reconcile(&input(vec![dep("40", true), dep("60", true)]))
                .unwrap()
                .confirmed_expected_total,
            a("100")
        );
    }
    #[test]
    fn underpayment_is_partial() {
        assert_eq!(
            reconcile(&input(vec![dep("60", true)])).unwrap().next_state,
            PaymentState::PartiallyPaid
        );
    }
    #[test]
    fn overpayment_requires_review_by_default() {
        assert_eq!(
            reconcile(&input(vec![dep("101", true)]))
                .unwrap()
                .next_state,
            PaymentState::ClaimPending
        );
    }

    #[test]
    fn explicit_accept_policy_records_overpayment() {
        let mut request = input(vec![dep("101", true)]);
        request.overpayment_policy = OverpaymentPolicy::AcceptAndRecord;
        assert_eq!(
            reconcile(&request).unwrap().next_state,
            PaymentState::Overpaid
        );
    }

    #[test]
    fn same_asset_on_the_wrong_chain_requires_review() {
        let mut wrong_chain = dep("100", true);
        wrong_chain.chain = "bsc_testnet".into();
        assert_eq!(
            reconcile(&input(vec![wrong_chain])).unwrap().next_state,
            PaymentState::WrongAsset
        );
    }
    #[test]
    fn nonfinal_stays_confirming() {
        assert_eq!(
            reconcile(&input(vec![dep("100", false)]))
                .unwrap()
                .next_state,
            PaymentState::Confirming
        );
    }
    #[test]
    fn wrong_token_is_not_payment() {
        let mut d = dep("100", true);
        d.asset_symbol = "USDT".into();
        d.token_contract = Some("0xusdt".into());
        assert_eq!(
            reconcile(&input(vec![d])).unwrap().next_state,
            PaymentState::WrongAsset
        );
    }

    #[test]
    fn mixed_assets_require_a_claim() {
        let mut wrong = dep("10", true);
        wrong.asset_symbol = "USDT".into();
        wrong.token_contract = Some("0xusdt".into());
        assert_eq!(
            reconcile(&input(vec![dep("100", true), wrong]))
                .unwrap()
                .next_state,
            PaymentState::ClaimPending
        );
    }

    #[test]
    fn every_payment_gets_a_distinct_chain_independent_salt() {
        let merchant = MerchantId(uuid::Uuid::from_u128(1));
        let first = PaymentId(uuid::Uuid::from_u128(2));
        let second = PaymentId(uuid::Uuid::from_u128(3));
        let first_salt = derive_checkout_salt(merchant, first);
        assert_ne!(first_salt, derive_checkout_salt(merchant, second));
        assert_eq!(first_salt, derive_checkout_salt(merchant, first));
    }
}

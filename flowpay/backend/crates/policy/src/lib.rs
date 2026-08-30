use flowpay_domain::{AtomicAmount, ChainKey, RecoveryPolicyDecision, RiskFlag};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SupportedAsset {
    pub chain: ChainKey,
    pub symbol: String,
    pub token_contract: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RecoveryPolicy {
    pub version: String,
    pub supported_chains: BTreeSet<String>,
    pub supported_assets: Vec<SupportedAsset>,
    pub minimum_recovery_amount: AtomicAmount,
    pub maximum_demo_recovery_amount: AtomicAmount,
    pub require_self_custody_signature: bool,
    pub require_simulation: bool,
    pub require_human_approval: bool,
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RecoveryPolicyInput {
    pub source_chain: ChainKey,
    pub asset_symbol: String,
    pub token_contract: Option<String>,
    pub amount: AtomicAmount,
    pub ownership_verified: bool,
    pub factory_verified: bool,
    pub funds_present: bool,
    pub gas_sufficient: bool,
    pub risk_flags: Vec<RiskFlag>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RecoveryPolicyResult {
    pub decision: RecoveryPolicyDecision,
    pub reasons: Vec<String>,
    pub required_approval: bool,
}

impl RecoveryPolicy {
    #[must_use]
    pub fn evaluate(&self, input: &RecoveryPolicyInput) -> RecoveryPolicyResult {
        let mut reasons = Vec::new();
        let chain_name = input.source_chain.to_string();

        if !self.supported_chains.contains(&chain_name) {
            reasons.push("unsupported_chain".to_owned());
        }

        let asset_supported = self.supported_assets.iter().any(|asset| {
            asset.chain == input.source_chain
                && asset.symbol.eq_ignore_ascii_case(&input.asset_symbol)
                && asset.token_contract == input.token_contract
        });
        if !asset_supported {
            reasons.push("unsupported_asset".to_owned());
        }
        if self.require_self_custody_signature && !input.ownership_verified {
            reasons.push("ownership_not_verified".to_owned());
        }
        if !input.factory_verified {
            reasons.push("factory_not_verified".to_owned());
        }
        if !input.funds_present {
            reasons.push("funds_not_present".to_owned());
        }
        if input.amount < self.minimum_recovery_amount {
            reasons.push("below_minimum_recovery_amount".to_owned());
        }
        if input.amount > self.maximum_demo_recovery_amount {
            reasons.push("above_demo_recovery_limit".to_owned());
        }

        if !reasons.is_empty() {
            return RecoveryPolicyResult {
                decision: RecoveryPolicyDecision::Denied,
                reasons,
                required_approval: false,
            };
        }

        if !input.gas_sufficient {
            return RecoveryPolicyResult {
                decision: RecoveryPolicyDecision::NeedsFunding,
                reasons: vec!["insufficient_execution_gas".to_owned()],
                required_approval: self.require_human_approval,
            };
        }

        if !input.risk_flags.is_empty() {
            return RecoveryPolicyResult {
                decision: RecoveryPolicyDecision::RequiresEscalation,
                reasons: vec!["risk_flags_present".to_owned()],
                required_approval: false,
            };
        }

        RecoveryPolicyResult {
            decision: RecoveryPolicyDecision::Allowed,
            reasons: vec!["policy_checks_passed".to_owned()],
            required_approval: self.require_human_approval,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn amount(value: &str) -> AtomicAmount {
        AtomicAmount::from_str(value).expect("valid amount")
    }

    fn policy() -> RecoveryPolicy {
        RecoveryPolicy {
            version: "demo-v1".to_owned(),
            supported_chains: ["bsc".to_owned()].into_iter().collect(),
            supported_assets: vec![SupportedAsset {
                chain: ChainKey::Bsc,
                symbol: "USDT".to_owned(),
                token_contract: Some("0xtest".to_owned()),
            }],
            minimum_recovery_amount: amount("1"),
            maximum_demo_recovery_amount: amount("1000000000"),
            require_self_custody_signature: true,
            require_simulation: true,
            require_human_approval: true,
        }
    }

    #[test]
    fn denies_unverified_ownership() {
        let result = policy().evaluate(&RecoveryPolicyInput {
            source_chain: ChainKey::Bsc,
            asset_symbol: "USDT".to_owned(),
            token_contract: Some("0xtest".to_owned()),
            amount: amount("50000000"),
            ownership_verified: false,
            factory_verified: true,
            funds_present: true,
            gas_sufficient: true,
            risk_flags: vec![],
        });
        assert_eq!(result.decision, RecoveryPolicyDecision::Denied);
        assert!(result
            .reasons
            .contains(&"ownership_not_verified".to_owned()));
    }

    #[test]
    fn valid_case_still_requires_human_approval() {
        let result = policy().evaluate(&RecoveryPolicyInput {
            source_chain: ChainKey::Bsc,
            asset_symbol: "USDT".to_owned(),
            token_contract: Some("0xtest".to_owned()),
            amount: amount("50000000"),
            ownership_verified: true,
            factory_verified: true,
            funds_present: true,
            gas_sufficient: true,
            risk_flags: vec![],
        });
        assert_eq!(result.decision, RecoveryPolicyDecision::Allowed);
        assert!(result.required_approval);
    }
}

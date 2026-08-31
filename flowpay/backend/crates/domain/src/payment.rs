use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PaymentState {
    Created,
    Waiting,
    Detected,
    Confirming,
    PartiallyPaid,
    Overpaid,
    WrongAsset,
    WrongChainClaimed,
    Confirmed,
    Settling,
    Completed,
    Expired,
    Failed,
    ClaimPending,
    RecoveryAvailable,
    RecoveryPending,
    Recovered,
    Escalated,
    Cancelled,
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
#[error("illegal payment state transition: {from:?} -> {to:?}")]
pub struct PaymentTransitionError {
    pub from: PaymentState,
    pub to: PaymentState,
}

#[allow(clippy::unnested_or_patterns)]
impl PaymentState {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Created => "CREATED",
            Self::Waiting => "WAITING",
            Self::Detected => "DETECTED",
            Self::Confirming => "CONFIRMING",
            Self::PartiallyPaid => "PARTIALLY_PAID",
            Self::Overpaid => "OVERPAID",
            Self::WrongAsset => "WRONG_ASSET",
            Self::WrongChainClaimed => "WRONG_CHAIN_CLAIMED",
            Self::Confirmed => "CONFIRMED",
            Self::Settling => "SETTLING",
            Self::Completed => "COMPLETED",
            Self::Expired => "EXPIRED",
            Self::Failed => "FAILED",
            Self::ClaimPending => "CLAIM_PENDING",
            Self::RecoveryAvailable => "RECOVERY_AVAILABLE",
            Self::RecoveryPending => "RECOVERY_PENDING",
            Self::Recovered => "RECOVERED",
            Self::Escalated => "ESCALATED",
            Self::Cancelled => "CANCELLED",
        }
    }

    #[must_use]
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Recovered | Self::Cancelled | Self::Escalated
        )
    }

    #[must_use]
    pub fn can_transition_to(self, to: Self) -> bool {
        use PaymentState as S;
        matches!(
            (self, to),
            (S::Created, S::Waiting)
                | (S::Created, S::Cancelled)
                | (S::Waiting, S::Detected)
                | (S::Waiting, S::Expired)
                | (S::Waiting, S::Cancelled)
                | (S::Waiting, S::ClaimPending)
                | (S::Detected, S::Confirming)
                | (S::Detected, S::Waiting) // explicit reorg rollback
                | (S::Detected, S::WrongAsset)
                | (S::Detected, S::ClaimPending)
                | (S::Confirming, S::Waiting) // all observed deposits were orphaned
                | (S::Confirming, S::WrongAsset) // expected deposit orphaned, unexpected asset remains
                | (S::Confirming, S::PartiallyPaid)
                | (S::Confirming, S::Overpaid)
                | (S::Confirming, S::Confirmed)
                | (S::Confirming, S::Failed)
                | (S::PartiallyPaid, S::Detected)
                | (S::PartiallyPaid, S::Waiting) // reorg removed confirmed partial deposits
                | (S::PartiallyPaid, S::Expired)
                | (S::PartiallyPaid, S::ClaimPending)
                | (S::Overpaid, S::Settling)
                | (S::Overpaid, S::Confirming) // canonicality rollback before settlement
                | (S::Overpaid, S::ClaimPending)
                | (S::WrongAsset, S::Waiting)
                | (S::WrongAsset, S::ClaimPending)
                | (S::Confirmed, S::Settling)
                | (S::Confirmed, S::Confirming) // canonicality rollback before settlement
                | (S::Confirmed, S::ClaimPending) // unexpected chain/asset or overpayment exception
                | (S::Settling, S::Completed)
                | (S::Settling, S::Failed)
                | (S::Failed, S::Settling)
                | (S::Failed, S::Escalated)
                | (S::Expired, S::ClaimPending)
                | (S::ClaimPending, S::WrongChainClaimed)
                | (S::ClaimPending, S::RecoveryAvailable)
                | (S::ClaimPending, S::Escalated)
                | (S::WrongChainClaimed, S::RecoveryAvailable)
                | (S::WrongChainClaimed, S::Escalated)
                | (S::RecoveryAvailable, S::RecoveryPending)
                | (S::RecoveryAvailable, S::Escalated)
                | (S::RecoveryPending, S::Recovered)
                | (S::RecoveryPending, S::Escalated)
        )
    }

    pub fn transition(self, to: Self) -> Result<Self, PaymentTransitionError> {
        self.can_transition_to(to)
            .then_some(to)
            .ok_or(PaymentTransitionError { from: self, to })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normal_path_is_legal() {
        let path = [
            PaymentState::Created,
            PaymentState::Waiting,
            PaymentState::Detected,
            PaymentState::Confirming,
            PaymentState::Confirmed,
            PaymentState::Settling,
            PaymentState::Completed,
        ];
        for pair in path.windows(2) {
            assert!(pair[0].can_transition_to(pair[1]));
        }
    }

    #[test]
    fn cannot_skip_verification_and_settlement() {
        assert!(!PaymentState::Waiting.can_transition_to(PaymentState::Completed));
        assert!(!PaymentState::Detected.can_transition_to(PaymentState::Completed));
    }

    #[test]
    fn partial_payment_can_accept_another_deposit() {
        assert!(PaymentState::PartiallyPaid.can_transition_to(PaymentState::Detected));
    }

    #[test]
    fn recovery_cannot_jump_directly_to_recovered() {
        assert!(!PaymentState::ClaimPending.can_transition_to(PaymentState::Recovered));
    }

    #[test]
    fn reorg_rollbacks_are_explicit_but_do_not_skip_forward_verification() {
        assert!(PaymentState::Confirmed.can_transition_to(PaymentState::Confirming));
        assert!(PaymentState::Overpaid.can_transition_to(PaymentState::Confirming));
        assert!(PaymentState::Confirming.can_transition_to(PaymentState::Waiting));
        assert!(!PaymentState::Confirmed.can_transition_to(PaymentState::Waiting));
    }
}

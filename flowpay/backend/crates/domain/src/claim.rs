use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ClaimState {
    Created,
    AwaitingEvidence,
    AwaitingAuthorization,
    Investigating,
    NeedsMoreEvidence,
    Recoverable,
    NotRecoverable,
    ApprovalPending,
    RecoveryPending,
    Recovered,
    Escalated,
    Rejected,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ClaimDisposition {
    Recoverable,
    NotRecoverable,
    NeedsMoreEvidence,
    Escalate,
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
#[error("illegal claim state transition: {from:?} -> {to:?}")]
pub struct ClaimTransitionError {
    pub from: ClaimState,
    pub to: ClaimState,
}

#[allow(clippy::unnested_or_patterns)]
impl ClaimState {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Created => "CREATED",
            Self::AwaitingEvidence => "AWAITING_EVIDENCE",
            Self::AwaitingAuthorization => "AWAITING_AUTHORIZATION",
            Self::Investigating => "INVESTIGATING",
            Self::NeedsMoreEvidence => "NEEDS_MORE_EVIDENCE",
            Self::Recoverable => "RECOVERABLE",
            Self::NotRecoverable => "NOT_RECOVERABLE",
            Self::ApprovalPending => "APPROVAL_PENDING",
            Self::RecoveryPending => "RECOVERY_PENDING",
            Self::Recovered => "RECOVERED",
            Self::Escalated => "ESCALATED",
            Self::Rejected => "REJECTED",
        }
    }

    #[must_use]
    pub fn can_transition_to(self, to: Self) -> bool {
        use ClaimState as S;
        matches!(
            (self, to),
            (S::Created, S::AwaitingEvidence)
                | (S::Created, S::AwaitingAuthorization)
                | (S::Created, S::Investigating)
                | (S::AwaitingEvidence, S::AwaitingAuthorization)
                | (S::AwaitingEvidence, S::Investigating)
                | (S::AwaitingEvidence, S::Rejected)
                | (S::AwaitingAuthorization, S::Investigating)
                | (S::AwaitingAuthorization, S::NeedsMoreEvidence)
                | (S::AwaitingAuthorization, S::Escalated)
                | (S::Investigating, S::NeedsMoreEvidence)
                | (S::Investigating, S::Recoverable)
                | (S::Investigating, S::NotRecoverable)
                | (S::Investigating, S::Escalated)
                | (S::NeedsMoreEvidence, S::AwaitingEvidence)
                | (S::NeedsMoreEvidence, S::Escalated)
                |            (S::Recoverable, S::RecoveryPending)
                | (S::Recoverable, S::ApprovalPending)
                | (S::Recoverable, S::Escalated)
                | (S::ApprovalPending, S::RecoveryPending)
                | (S::ApprovalPending, S::Escalated)
                | (S::RecoveryPending, S::Recovered)
                | (S::RecoveryPending, S::Escalated)
        )
    }

    pub fn transition(self, to: Self) -> Result<Self, ClaimTransitionError> {
        self.can_transition_to(to)
            .then_some(to)
            .ok_or(ClaimTransitionError { from: self, to })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proven_recovery_skips_approval() {
        // Proven recoveries go directly from Recoverable to RecoveryPending
        assert!(ClaimState::Recoverable.can_transition_to(ClaimState::RecoveryPending));
        // The approval path still exists for edge cases
        assert!(ClaimState::Recoverable.can_transition_to(ClaimState::ApprovalPending));
        assert!(ClaimState::ApprovalPending.can_transition_to(ClaimState::RecoveryPending));
    }
}

use crate::{AddressRef, AtomicAmount, ChainKey, ClaimId, PaymentId, RecoveryPlanId};
use num_bigint::BigUint;
use serde::{Deserialize, Serialize};

/// Platform recovery fee in basis points (1000 = 10%).
pub const RECOVERY_FEE_BPS: u64 = 1000;

/// Compute the owner-receivable amount after deducting the platform fee.
/// Returns `total * (10000 - RECOVERY_FEE_BPS) / 10000`.
pub fn owner_receivable_amount(total: &AtomicAmount) -> AtomicAmount {
    let owner_bps = BigUint::from(10000u64 - RECOVERY_FEE_BPS);
    let divisor = BigUint::from(10000u64);
    let net = total.inner() * owner_bps / divisor;
    AtomicAmount::from_biguint(net)
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SimulationStatus {
    NotRun,
    Succeeded,
    Failed,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RiskFlag {
    CustodialSender,
    FactoryMismatch,
    UnsupportedTokenBehavior,
    BalanceChanged,
    RpcInconsistency,
    AmbiguousOwnership,
    CrossChain,
    AmountMismatch,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RecoveryPolicyDecision {
    Allowed,
    Denied,
    RequiresEscalation,
    NeedsFunding,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RecoveryPlan {
    pub id: RecoveryPlanId,
    pub claim_id: ClaimId,
    pub payment_id: PaymentId,
    pub source_chain: ChainKey,
    pub token_contract: Option<String>,
    pub asset_symbol: String,
    pub amount: AtomicAmount,
    pub checkout_address: AddressRef,
    pub recovery_destination: AddressRef,
    pub receiver_deployment_required: bool,
    pub estimated_gas_atomic: AtomicAmount,
    pub policy_version: String,
    pub policy_decision: RecoveryPolicyDecision,
    pub simulation_status: SimulationStatus,
    pub risk_flags: Vec<RiskFlag>,
    pub required_approval: bool,
}

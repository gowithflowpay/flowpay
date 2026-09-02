use alloy_primitives::keccak256;
use flowpay_chains::PreparedTransaction;
use flowpay_domain::{
    AddressRef, AtomicAmount, ClaimId, PaymentId, RecoveryPlan, RecoveryPlanId,
    RecoveryPolicyDecision, RiskFlag, SimulationStatus,
};
use flowpay_policy::{RecoveryPolicy, RecoveryPolicyInput, RecoveryPolicyResult};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RecoveryFacts {
    pub claim_id: ClaimId,
    pub payment_id: PaymentId,
    pub source_chain: flowpay_domain::ChainKey,
    pub asset_symbol: String,
    pub token_contract: Option<String>,
    pub amount: AtomicAmount,
    pub checkout_address: AddressRef,
    pub recovery_destination: AddressRef,
    pub receiver_deployment_required: bool,
    pub estimated_gas_atomic: AtomicAmount,
    pub ownership_verified: bool,
    pub factory_verified: bool,
    pub funds_present: bool,
    pub gas_sufficient: bool,
    pub risk_flags: Vec<RiskFlag>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct HashedRecoveryPlan {
    pub plan: RecoveryPlan,
    pub canonical_json: String,
    pub plan_hash: String,
}

#[derive(Debug, Error)]
pub enum RecoveryError {
    #[error("policy did not allow recovery: {0:?}")]
    PolicyDenied(RecoveryPolicyDecision),
    #[error("invalid EVM address")]
    InvalidAddress,
    #[error("token address required for ERC20 recovery")]
    MissingToken,
}

pub fn build_plan(
    policy: &RecoveryPolicy,
    facts: RecoveryFacts,
) -> (HashedRecoveryPlan, RecoveryPolicyResult) {
    let policy_result = policy.evaluate(&RecoveryPolicyInput {
        source_chain: facts.source_chain.clone(),
        asset_symbol: facts.asset_symbol.clone(),
        token_contract: facts.token_contract.clone(),
        amount: facts.amount.clone(),
        ownership_verified: facts.ownership_verified,
        factory_verified: facts.factory_verified,
        funds_present: facts.funds_present,
        gas_sufficient: facts.gas_sufficient,
        risk_flags: facts.risk_flags.clone(),
    });
    let plan = RecoveryPlan {
        id: RecoveryPlanId::new(),
        claim_id: facts.claim_id,
        payment_id: facts.payment_id,
        source_chain: facts.source_chain,
        token_contract: facts.token_contract,
        asset_symbol: facts.asset_symbol,
        amount: facts.amount,
        checkout_address: facts.checkout_address,
        recovery_destination: facts.recovery_destination,
        receiver_deployment_required: facts.receiver_deployment_required,
        estimated_gas_atomic: facts.estimated_gas_atomic,
        policy_version: policy.version.clone(),
        policy_decision: policy_result.decision.clone(),
        simulation_status: SimulationStatus::NotRun,
        risk_flags: facts.risk_flags,
        required_approval: policy_result.required_approval,
    };
    let canonical_json =
        serde_json::to_string(&plan).expect("RecoveryPlan serialization cannot fail");
    let plan_hash = format!(
        "sha256:{}",
        hex::encode(Sha256::digest(canonical_json.as_bytes()))
    );
    (
        HashedRecoveryPlan {
            plan,
            canonical_json,
            plan_hash,
        },
        policy_result,
    )
}

#[must_use]
pub fn with_simulation(mut hashed: HashedRecoveryPlan, success: bool) -> HashedRecoveryPlan {
    hashed.plan.simulation_status = if success {
        SimulationStatus::Succeeded
    } else {
        SimulationStatus::Failed
    };
    hashed.canonical_json =
        serde_json::to_string(&hashed.plan).expect("RecoveryPlan serialization cannot fail");
    hashed.plan_hash = format!(
        "sha256:{}",
        hex::encode(Sha256::digest(hashed.canonical_json.as_bytes()))
    );
    hashed
}

fn encode_address(value: &str) -> Result<[u8; 32], RecoveryError> {
    let raw =
        hex::decode(value.trim_start_matches("0x")).map_err(|_| RecoveryError::InvalidAddress)?;
    if raw.len() != 20 {
        return Err(RecoveryError::InvalidAddress);
    }
    let mut out = [0_u8; 32];
    out[12..].copy_from_slice(&raw);
    Ok(out)
}

fn encode_uint256(value: &AtomicAmount) -> Result<[u8; 32], RecoveryError> {
    let bytes = value.inner().to_bytes_be();
    if bytes.len() > 32 {
        return Err(RecoveryError::InvalidAddress);
    }
    let mut out = [0_u8; 32];
    out[32 - bytes.len()..].copy_from_slice(&bytes);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use flowpay_domain::{ChainKey, MerchantId};
    use flowpay_policy::SupportedAsset;
    use std::{collections::BTreeSet, str::FromStr};

    fn a(v: &str) -> AtomicAmount {
        AtomicAmount::from_str(v).unwrap()
    }
    #[test]
    fn plan_hash_changes_when_destination_changes() {
        let policy = RecoveryPolicy {
            version: "v1".into(),
            supported_chains: BTreeSet::from(["bsc".into()]),
            supported_assets: vec![SupportedAsset {
                chain: ChainKey::Bsc,
                symbol: "USDT".into(),
                token_contract: Some("0x1111111111111111111111111111111111111111".into()),
            }],
            minimum_recovery_amount: a("1"),
            maximum_demo_recovery_amount: a("100000000"),
            require_self_custody_signature: true,
            require_simulation: true,
            require_human_approval: true,
        };
        let payment_id = PaymentId::new();
        let claim_id = ClaimId::new();
        let _merchant = MerchantId::new();
        let facts = RecoveryFacts {
            claim_id,
            payment_id,
            source_chain: ChainKey::Bsc,
            asset_symbol: "USDT".into(),
            token_contract: Some("0x1111111111111111111111111111111111111111".into()),
            amount: a("50000000"),
            checkout_address: AddressRef {
                chain: ChainKey::Bsc,
                value: "0x2222222222222222222222222222222222222222".into(),
            },
            recovery_destination: AddressRef {
                chain: ChainKey::Bsc,
                value: "0x3333333333333333333333333333333333333333".into(),
            },
            receiver_deployment_required: true,
            estimated_gas_atomic: a("200000"),
            ownership_verified: true,
            factory_verified: true,
            funds_present: true,
            gas_sufficient: true,
            risk_flags: vec![],
        };
        let (first, _) = build_plan(&policy, facts.clone());
        let mut changed = facts;
        changed.recovery_destination.value = "0x4444444444444444444444444444444444444444".into();
        let (second, _) = build_plan(&policy, changed);
        assert_ne!(first.plan_hash, second.plan_hash);
    }

    #[test]
    fn builds_native_recovery_for_a_verified_plan() {
        let policy = RecoveryPolicy {
            version: "v1".into(),
            supported_chains: BTreeSet::from(["base_sepolia".into()]),
            supported_assets: vec![SupportedAsset {
                chain: ChainKey::Custom("base_sepolia".into()),
                symbol: "ETH".into(),
                token_contract: None,
            }],
            minimum_recovery_amount: a("1"),
            maximum_demo_recovery_amount: a("1000000000000000000"),
            require_self_custody_signature: true,
            require_simulation: true,
            require_human_approval: true,
        };
        let chain = ChainKey::Custom("base_sepolia".into());
        let (mut hashed, result) = build_plan(
            &policy,
            RecoveryFacts {
                claim_id: ClaimId::new(),
                payment_id: PaymentId::new(),
                source_chain: chain.clone(),
                asset_symbol: "ETH".into(),
                token_contract: None,
                amount: a("100000000000000"),
                checkout_address: AddressRef {
                    chain: chain.clone(),
                    value: "0x2222222222222222222222222222222222222222".into(),
                },
                recovery_destination: AddressRef {
                    chain,
                    value: "0x3333333333333333333333333333333333333333".into(),
                },
                receiver_deployment_required: true,
                estimated_gas_atomic: a("300000"),
                ownership_verified: true,
                factory_verified: true,
                funds_present: true,
                gas_sufficient: true,
                risk_flags: vec![RiskFlag::CrossChain],
            },
        );
        assert_eq!(result.decision, RecoveryPolicyDecision::Allowed);
        hashed.plan.simulation_status = SimulationStatus::Succeeded;
        let tx = build_live_recovery_transaction(
            &hashed.plan,
            [9_u8; 32],
            "0x1111111111111111111111111111111111111111",
        )
        .unwrap();
        assert_eq!(tx.transaction_class, "RECOVER_NATIVE");
        assert_eq!(tx.calldata_hex.len(), 2 + 8 + 64 * 3);
    }
}

pub fn build_live_recover_erc20_transaction(
    plan: &RecoveryPlan,
    stored_checkout_salt: [u8; 32],
    factory: &str,
) -> Result<PreparedTransaction, RecoveryError> {
    if !matches!(
        plan.policy_decision,
        RecoveryPolicyDecision::Allowed | RecoveryPolicyDecision::NeedsFunding
    ) || plan.simulation_status != SimulationStatus::Succeeded
    {
        return Err(RecoveryError::PolicyDenied(plan.policy_decision.clone()));
    }
    let token = plan
        .token_contract
        .as_deref()
        .ok_or(RecoveryError::MissingToken)?;
    let selector = &keccak256("recoverToken(bytes32,address,address,uint256)".as_bytes())[..4];
    let mut calldata = Vec::with_capacity(132);
    calldata.extend_from_slice(selector);
    calldata.extend_from_slice(&stored_checkout_salt);
    calldata.extend_from_slice(&encode_address(token)?);
    calldata.extend_from_slice(&encode_address(&plan.recovery_destination.value)?);
    calldata.extend_from_slice(&encode_uint256(&plan.amount)?);
    Ok(PreparedTransaction {
        transaction_class: "RECOVER_ERC20".into(),
        chain: plan.source_chain.clone(),
        to: factory.to_owned(),
        value: AtomicAmount::zero(),
        calldata_hex: format!("0x{}", hex::encode(calldata)),
    })
}

pub fn build_live_recovery_transaction(
    plan: &RecoveryPlan,
    stored_checkout_salt: [u8; 32],
    factory: &str,
) -> Result<PreparedTransaction, RecoveryError> {
    if plan.token_contract.is_some() {
        build_live_recover_erc20_transaction(plan, stored_checkout_salt, factory)
    } else {
        if !matches!(
            plan.policy_decision,
            RecoveryPolicyDecision::Allowed | RecoveryPolicyDecision::NeedsFunding
        ) || plan.simulation_status != SimulationStatus::Succeeded
        {
            return Err(RecoveryError::PolicyDenied(plan.policy_decision.clone()));
        }
        build_factory_native_sweep(
            plan.source_chain.clone(),
            stored_checkout_salt,
            factory,
            &plan.recovery_destination.value,
            &plan.amount,
            "RECOVER_NATIVE",
        )
    }
}

pub fn build_factory_erc20_sweep(
    chain: flowpay_domain::ChainKey,
    stored_checkout_salt: [u8; 32],
    factory: &str,
    token: &str,
    destination: &str,
    amount: &AtomicAmount,
    transaction_class: &str,
) -> Result<PreparedTransaction, RecoveryError> {
    let selector = &keccak256("recoverToken(bytes32,address,address,uint256)".as_bytes())[..4];
    let mut calldata = Vec::with_capacity(132);
    calldata.extend_from_slice(selector);
    calldata.extend_from_slice(&stored_checkout_salt);
    calldata.extend_from_slice(&encode_address(token)?);
    calldata.extend_from_slice(&encode_address(destination)?);
    calldata.extend_from_slice(&encode_uint256(amount)?);
    Ok(PreparedTransaction {
        transaction_class: transaction_class.to_owned(),
        chain,
        to: factory.to_owned(),
        value: AtomicAmount::zero(),
        calldata_hex: format!("0x{}", hex::encode(calldata)),
    })
}

pub fn build_factory_native_sweep(
    chain: flowpay_domain::ChainKey,
    stored_checkout_salt: [u8; 32],
    factory: &str,
    destination: &str,
    amount: &AtomicAmount,
    transaction_class: &str,
) -> Result<PreparedTransaction, RecoveryError> {
    let selector = &keccak256("recoverNative(bytes32,address,uint256)".as_bytes())[..4];
    let mut calldata = Vec::with_capacity(100);
    calldata.extend_from_slice(selector);
    calldata.extend_from_slice(&stored_checkout_salt);
    calldata.extend_from_slice(&encode_address(destination)?);
    calldata.extend_from_slice(&encode_uint256(amount)?);
    Ok(PreparedTransaction {
        transaction_class: transaction_class.to_owned(),
        chain,
        to: factory.to_owned(),
        value: AtomicAmount::zero(),
        calldata_hex: format!("0x{}", hex::encode(calldata)),
    })
}

pub mod model;

use async_trait::async_trait;
use flowpay_domain::{
    AddressRef, ApprovalId, AtomicAmount, ChainKey, ClaimDisposition, ClaimId, PaymentId,
    RecoveryPlan, RecoveryPlanId,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AgentContext {
    pub agent_run_id: String,
    pub merchant_id: String,
    pub claim_id: ClaimId,
    pub payment_id: PaymentId,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PaymentSnapshot {
    pub payment_id: PaymentId,
    pub expected_chain: ChainKey,
    pub expected_asset: String,
    pub expected_amount: AtomicAmount,
    pub checkout_address: AddressRef,
    pub current_state: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ClaimSnapshot {
    pub claim_id: ClaimId,
    pub claimed_chain: Option<ChainKey>,
    pub transaction_hash: Option<String>,
    pub requested_destination: AddressRef,
    pub explanation: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct VerifiedTransaction {
    pub chain: ChainKey,
    pub hash: String,
    pub from: String,
    pub to: String,
    pub token_contract: Option<String>,
    pub amount: AtomicAmount,
    pub success: bool,
    pub canonical: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct WalletAuthorizationResult {
    pub verified: bool,
    pub wallet: Option<String>,
    pub reason: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CounterfactualVerification {
    pub matches: bool,
    pub predicted_address: String,
    pub factory_verified: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ToolDecision {
    pub disposition: ClaimDisposition,
    pub concise_rationale: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ApprovalRequestResult {
    pub approval_id: ApprovalId,
    pub status: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RecoveryExecutionResult {
    pub transaction_hash: String,
    pub submitted: bool,
}

#[derive(Debug, Error)]
pub enum ToolError {
    #[error("not authorized")]
    NotAuthorized,
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("policy denied: {0}")]
    PolicyDenied(String),
    #[error("retryable dependency failure: {0}")]
    Retryable(String),
    #[error("permanent dependency failure: {0}")]
    Permanent(String),
}

#[async_trait]
pub trait InvestigationTools: Send + Sync {
    async fn get_payment(&self, ctx: &AgentContext) -> Result<PaymentSnapshot, ToolError>;
    async fn get_claim(&self, ctx: &AgentContext) -> Result<ClaimSnapshot, ToolError>;
    async fn verify_wallet_signature(
        &self,
        ctx: &AgentContext,
    ) -> Result<WalletAuthorizationResult, ToolError>;
    async fn get_transaction(
        &self,
        ctx: &AgentContext,
        chain: ChainKey,
        transaction_hash: &str,
    ) -> Result<VerifiedTransaction, ToolError>;
    async fn verify_counterfactual_address(
        &self,
        ctx: &AgentContext,
        chain: ChainKey,
        candidate_address: &str,
    ) -> Result<CounterfactualVerification, ToolError>;
    async fn get_token_balance(
        &self,
        ctx: &AgentContext,
        chain: ChainKey,
        token_contract: &str,
        address: &str,
    ) -> Result<AtomicAmount, ToolError>;
    async fn build_recovery_plan(&self, ctx: &AgentContext) -> Result<RecoveryPlan, ToolError>;
    async fn simulate_recovery(
        &self,
        ctx: &AgentContext,
        plan_id: RecoveryPlanId,
    ) -> Result<bool, ToolError>;
}

#[async_trait]
pub trait ControlledRecoveryTools: Send + Sync {
    async fn request_approval(
        &self,
        ctx: &AgentContext,
        plan_id: RecoveryPlanId,
    ) -> Result<ApprovalRequestResult, ToolError>;

    /// Auto-approve and execute a proven recovery without human approval.
    /// Deducts the platform fee (10%) and sends the net amount to the owner.
    async fn execute_proven_recovery(
        &self,
        ctx: &AgentContext,
        plan_id: RecoveryPlanId,
    ) -> Result<RecoveryExecutionResult, ToolError>;

    async fn execute_approved_recovery(
        &self,
        ctx: &AgentContext,
        plan_id: RecoveryPlanId,
        approval_id: ApprovalId,
    ) -> Result<RecoveryExecutionResult, ToolError>;

    async fn verify_recovery(
        &self,
        ctx: &AgentContext,
        plan_id: RecoveryPlanId,
        transaction_hash: &str,
    ) -> Result<bool, ToolError>;
}

// The agent never receives a generic `sign`, `send_raw_transaction`, or arbitrary calldata tool.

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TrajectoryStep {
    pub sequence: u32,
    pub tool: String,
    pub input_summary: serde_json::Value,
    pub output_summary: serde_json::Value,
    pub verification: String,
    pub rationale: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AgentRunStatus {
    RecoverableAwaitingApproval,
    RecoverableAwaitingFunding,
    NeedsMoreEvidence,
    NotRecoverable,
    Escalated,
    Recovered,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AgentRunResult {
    pub status: AgentRunStatus,
    pub concise_rationale: String,
    pub plan_id: Option<RecoveryPlanId>,
    pub approval_id: Option<ApprovalId>,
    pub recovery_transaction_hash: Option<String>,
    pub trajectory: Vec<TrajectoryStep>,
}

pub struct SafetyFirstAgent<T> {
    tools: T,
}

impl<T> SafetyFirstAgent<T>
where
    T: InvestigationTools + ControlledRecoveryTools,
{
    pub fn new(tools: T) -> Self {
        Self { tools }
    }

    pub async fn investigate(&self, ctx: &AgentContext) -> Result<AgentRunResult, ToolError> {
        let mut trace = Vec::new();
        let mut seq = 1_u32;
        let payment = self.tools.get_payment(ctx).await?;
        trace.push(step(seq,"get_payment",serde_json::json!({"payment_id":ctx.payment_id}),serde_json::json!({"expected_chain":payment.expected_chain,"expected_asset":payment.expected_asset,"checkout_address":payment.checkout_address}),"payment loaded from authoritative store","Need immutable expected-payment facts before considering claim evidence."));
        seq += 1;
        let claim = self.tools.get_claim(ctx).await?;
        trace.push(step(seq,"get_claim",serde_json::json!({"claim_id":ctx.claim_id}),serde_json::json!({"claimed_chain":claim.claimed_chain,"transaction_hash":claim.transaction_hash,"requested_destination":claim.requested_destination}),"claim data treated as untrusted leads","Claim supplies where to investigate, not financial truth."));
        seq += 1;
        let auth = self.tools.verify_wallet_signature(ctx).await?;
        trace.push(step(
            seq,
            "verify_wallet_signature",
            serde_json::json!({"claim_id":ctx.claim_id}),
            serde_json::json!({"verified":auth.verified,"wallet":auth.wallet}),
            &auth.reason,
            "Automatic recovery is limited to cryptographically authorized self-custody claims.",
        ));
        seq += 1;
        if !auth.verified {
            return Ok(AgentRunResult { status:AgentRunStatus::NeedsMoreEvidence, concise_rationale:"wallet ownership is not cryptographically verified; automatic recovery is unsafe".into(),plan_id:None,approval_id:None,recovery_transaction_hash:None,trajectory:trace });
        }
        let Some(chain) = claim.claimed_chain.clone() else {
            return Ok(AgentRunResult {
                status: AgentRunStatus::NeedsMoreEvidence,
                concise_rationale: "claimed network is missing".into(),
                plan_id: None,
                approval_id: None,
                recovery_transaction_hash: None,
                trajectory: trace,
            });
        };
        let Some(tx_hash) = claim.transaction_hash.as_deref() else {
            return Ok(AgentRunResult {
                status: AgentRunStatus::NeedsMoreEvidence,
                concise_rationale: "transaction hash is missing".into(),
                plan_id: None,
                approval_id: None,
                recovery_transaction_hash: None,
                trajectory: trace,
            });
        };
        let tx = match self
            .tools
            .get_transaction(ctx, chain.clone(), tx_hash)
            .await
        {
            Ok(tx) => tx,
            Err(ToolError::NotFound(_)) => {
                return Ok(AgentRunResult {
                    status: AgentRunStatus::NotRecoverable,
                    concise_rationale: "claimed transaction could not be independently found"
                        .into(),
                    plan_id: None,
                    approval_id: None,
                    recovery_transaction_hash: None,
                    trajectory: trace,
                })
            }
            Err(e) => return Err(e),
        };
        trace.push(step(seq,"get_transaction",serde_json::json!({"chain":chain,"transaction_hash":tx_hash}),serde_json::json!({"from":tx.from,"to":tx.to,"token_contract":tx.token_contract,"amount":tx.amount,"success":tx.success,"canonical":tx.canonical}),"transaction independently queried from chain adapter","A user-supplied hash is not accepted without chain verification."));
        seq += 1;
        if auth
            .wallet
            .as_deref()
            .is_some_and(|wallet| !wallet.eq_ignore_ascii_case(&tx.from))
        {
            return Ok(AgentRunResult {
                status: AgentRunStatus::NeedsMoreEvidence,
                concise_rationale:
                    "authorized wallet does not match the verified transaction sender".into(),
                plan_id: None,
                approval_id: None,
                recovery_transaction_hash: None,
                trajectory: trace,
            });
        }
        if !tx.success || !tx.canonical {
            return Ok(AgentRunResult {
                status: AgentRunStatus::NotRecoverable,
                concise_rationale: "transaction is reverted or non-canonical".into(),
                plan_id: None,
                approval_id: None,
                recovery_transaction_hash: None,
                trajectory: trace,
            });
        }
        let cf = self
            .tools
            .verify_counterfactual_address(ctx, chain.clone(), &tx.to)
            .await?;
        trace.push(step(seq,"verify_counterfactual_address",serde_json::json!({"chain":chain,"candidate_address":tx.to}),serde_json::json!({"matches":cf.matches,"predicted_address":cf.predicted_address,"factory_verified":cf.factory_verified}),"factory and deterministic address relationship checked","Recovery is forbidden if the claimed chain cannot reproduce FlowPay control of the address."));
        seq += 1;
        if !cf.matches || !cf.factory_verified {
            return Ok(AgentRunResult {
                status: AgentRunStatus::Escalated,
                concise_rationale:
                    "counterfactual checkout/factory invariant could not be verified".into(),
                plan_id: None,
                approval_id: None,
                recovery_transaction_hash: None,
                trajectory: trace,
            });
        }
        let Some(token) = tx.token_contract.as_deref() else {
            return Ok(AgentRunResult {
                status: AgentRunStatus::Escalated,
                concise_rationale: "native-asset recovery requires a separate constrained path"
                    .into(),
                plan_id: None,
                approval_id: None,
                recovery_transaction_hash: None,
                trajectory: trace,
            });
        };
        let balance = self
            .tools
            .get_token_balance(ctx, chain.clone(), token, &tx.to)
            .await?;
        trace.push(step(
            seq,
            "get_token_balance",
            serde_json::json!({"chain":chain,"token_contract":token,"address":tx.to}),
            serde_json::json!({"balance":balance}),
            "current recoverable balance checked on-chain",
            "A historical deposit is not recoverable if funds are no longer present.",
        ));
        seq += 1;
        if balance.is_zero() {
            return Ok(AgentRunResult {
                status: AgentRunStatus::NotRecoverable,
                concise_rationale: "funds are no longer present at the checkout address".into(),
                plan_id: None,
                approval_id: None,
                recovery_transaction_hash: None,
                trajectory: trace,
            });
        }
        let plan = self.tools.build_recovery_plan(ctx).await?;
        trace.push(step(seq,"build_recovery_plan",serde_json::json!({"claim_id":ctx.claim_id}),serde_json::json!({"plan_id":plan.id,"policy_decision":plan.policy_decision,"required_approval":plan.required_approval,"risk_flags":plan.risk_flags}),"deterministic recovery policy applied","The agent can only propose a predefined RecoveryPlan."));
        seq += 1;
        match plan.policy_decision {
            flowpay_domain::RecoveryPolicyDecision::Denied => return Ok(AgentRunResult { status:AgentRunStatus::NotRecoverable, concise_rationale:"recovery policy does not support this chain/asset/amount or ownership condition".into(),plan_id:Some(plan.id),approval_id:None,recovery_transaction_hash:None,trajectory:trace }),
            flowpay_domain::RecoveryPolicyDecision::RequiresEscalation => return Ok(AgentRunResult { status:AgentRunStatus::Escalated, concise_rationale:"recovery policy requires manual escalation because risk flags are present".into(),plan_id:Some(plan.id),approval_id:None,recovery_transaction_hash:None,trajectory:trace }),
            flowpay_domain::RecoveryPolicyDecision::Allowed | flowpay_domain::RecoveryPolicyDecision::NeedsFunding => {}
        }
        let simulated = self.tools.simulate_recovery(ctx, plan.id).await?;
        trace.push(step(
            seq,
            "simulate_recovery",
            serde_json::json!({"plan_id":plan.id}),
            serde_json::json!({"success":simulated}),
            "transaction simulation must succeed before approval",
            "Simulation is a hard gate, not advisory.",
        ));
        seq += 1;
        if !simulated {
            return Ok(AgentRunResult {
                status: AgentRunStatus::Escalated,
                concise_rationale: "recovery simulation failed".into(),
                plan_id: Some(plan.id),
                approval_id: None,
                recovery_transaction_hash: None,
                trajectory: trace,
            });
        }
        if plan.policy_decision == flowpay_domain::RecoveryPolicyDecision::NeedsFunding {
            return Ok(AgentRunResult { status:AgentRunStatus::RecoverableAwaitingFunding, concise_rationale:"recovery is technically valid and simulation passed, but the restricted signer lacks test gas".into(),plan_id:Some(plan.id),approval_id:None,recovery_transaction_hash:None,trajectory:trace });
        }
        if plan.required_approval {
            let approval = self.tools.request_approval(ctx, plan.id).await?;
            trace.push(step(
                seq,
                "request_approval",
                serde_json::json!({"plan_id":plan.id}),
                serde_json::json!({"approval_id":approval.approval_id,"status":approval.status}),
                "deterministic policy requires a human approval checkpoint",
                "Cross-chain and amount-discrepancy refunds are never auto-executed.",
            ));
            return Ok(AgentRunResult {
                status: AgentRunStatus::RecoverableAwaitingApproval,
                concise_rationale:
                    "recovery passed verification and simulation and is waiting for human approval"
                        .into(),
                plan_id: Some(plan.id),
                approval_id: Some(approval.approval_id),
                recovery_transaction_hash: None,
                trajectory: trace,
            });
        }
        // Proven low-risk recovery: auto-execute only after all deterministic gates pass.
        // 10% platform fee is deducted; net amount returned to owner.
        let execution = self.tools.execute_proven_recovery(ctx, plan.id).await?;
        trace.push(step(
            seq,
            "execute_proven_recovery",
            serde_json::json!({"plan_id":plan.id}),
            serde_json::json!({"submitted":execution.submitted,"transaction_hash":execution.transaction_hash}),
            "proven recovery auto-executed: 10% fee deducted, net sent to owner",
            "All deterministic gates passed. No human approval required for proven claims.",
        ));
        if !execution.submitted {
            return Ok(AgentRunResult {
                status: AgentRunStatus::Escalated,
                concise_rationale: "proven recovery execution failed".into(),
                plan_id: Some(plan.id),
                approval_id: None,
                recovery_transaction_hash: None,
                trajectory: trace,
            });
        }
        seq += 1;
        let verified = self
            .tools
            .verify_recovery(ctx, plan.id, &execution.transaction_hash)
            .await?;
        trace.push(step(
            seq,
            "verify_recovery",
            serde_json::json!({"transaction_hash":execution.transaction_hash}),
            serde_json::json!({"verified":verified}),
            "receipt and balance delta verified",
            "Submission alone is not success.",
        ));
        Ok(AgentRunResult {
            status: if verified {
                AgentRunStatus::Recovered
            } else {
                AgentRunStatus::Escalated
            },
            concise_rationale: if verified {
                "proven recovery executed and verified; 10% fee deducted".into()
            } else {
                "recovery could not be independently verified".into()
            },
            plan_id: Some(plan.id),
            approval_id: None,
            recovery_transaction_hash: Some(execution.transaction_hash),
            trajectory: trace,
        })
    }

    pub async fn execute_after_approval(
        &self,
        ctx: &AgentContext,
        plan_id: RecoveryPlanId,
        approval_id: ApprovalId,
    ) -> Result<AgentRunResult, ToolError> {
        let execution = self
            .tools
            .execute_approved_recovery(ctx, plan_id, approval_id)
            .await?;
        let mut trace = vec![step(
            1,
            "execute_approved_recovery",
            serde_json::json!({"plan_id":plan_id,"approval_id":approval_id}),
            serde_json::json!({"submitted":execution.submitted,"transaction_hash":execution.transaction_hash}),
            "restricted signer validates approval and allowlisted transaction",
            "The agent never receives signing material or arbitrary calldata authority.",
        )];
        if !execution.submitted {
            return Ok(AgentRunResult {
                status: AgentRunStatus::Escalated,
                concise_rationale: "approved recovery was not submitted".into(),
                plan_id: Some(plan_id),
                approval_id: Some(approval_id),
                recovery_transaction_hash: None,
                trajectory: trace,
            });
        }
        let verified = self
            .tools
            .verify_recovery(ctx, plan_id, &execution.transaction_hash)
            .await?;
        trace.push(step(
            2,
            "verify_recovery",
            serde_json::json!({"transaction_hash":execution.transaction_hash}),
            serde_json::json!({"verified":verified}),
            "receipt and resulting balance state independently verified",
            "Submission alone is not success.",
        ));
        Ok(AgentRunResult {
            status: if verified {
                AgentRunStatus::Recovered
            } else {
                AgentRunStatus::Escalated
            },
            concise_rationale: if verified {
                "approved recovery executed and receipt/balance verification passed".into()
            } else {
                "recovery transaction could not be verified".into()
            },
            plan_id: Some(plan_id),
            approval_id: Some(approval_id),
            recovery_transaction_hash: Some(execution.transaction_hash),
            trajectory: trace,
        })
    }
}

fn step(
    sequence: u32,
    tool: &str,
    input_summary: serde_json::Value,
    output_summary: serde_json::Value,
    verification: &str,
    rationale: &str,
) -> TrajectoryStep {
    TrajectoryStep {
        sequence,
        tool: tool.into(),
        input_summary,
        output_summary,
        verification: verification.into(),
        rationale: rationale.into(),
    }
}

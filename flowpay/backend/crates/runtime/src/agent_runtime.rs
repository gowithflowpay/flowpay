use crate::state::AppState;
use async_trait::async_trait;
use flowpay_agent::{
    AgentContext, ApprovalRequestResult, ClaimSnapshot, ControlledRecoveryTools,
    CounterfactualVerification, InvestigationTools, PaymentSnapshot, RecoveryExecutionResult,
    ToolError, VerifiedTransaction, WalletAuthorizationResult,
};
use flowpay_chains::{ChainAdapter, CounterfactualEvmAdapter};
use flowpay_domain::{
    AddressRef, ApprovalId, AtomicAmount, ChainKey, ClaimState, PaymentState, RecoveryPlan,
    RecoveryPlanId, RecoveryPolicyDecision, RiskFlag, SimulationStatus,
};
use flowpay_policy::{RecoveryPolicy, SupportedAsset};
use flowpay_recovery::{
    build_live_recover_erc20_transaction, build_plan, with_simulation, HashedRecoveryPlan,
    RecoveryFacts,
};
use flowpay_signer::{
    ApprovedSignerRequest, DevUnlockedSigner, RestrictedSigner, SignerPolicy, TestnetKeySigner,
    TransactionClass,
};
use serde_json::{json, Value};
use sqlx::Row;
use std::{collections::BTreeSet, str::FromStr};
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

#[derive(Clone)]
pub struct DatabaseAgentTools {
    pub state: AppState,
}

impl DatabaseAgentTools {
    pub fn new(state: AppState) -> Self {
        Self { state }
    }

    async fn verified_transfer(
        &self,
        ctx: &AgentContext,
        chain: &ChainKey,
        tx_hash: &str,
    ) -> Result<VerifiedTransaction, ToolError> {
        let runtime = self
            .state
            .chains
            .get(chain)
            .ok_or_else(|| ToolError::Permanent("unsupported network".into()))?;
        let payment = self
            .state
            .store
            .get_payment_by_id(ctx.payment_id)
            .await
            .map_err(store_err)?;
        let salt = self
            .state
            .store
            .checkout_salt(payment.id, &payment.expected_chain)
            .await
            .map_err(store_err)?;
        let predicted = runtime.deriver.checkout_hex(salt);
        let raw = runtime
            .adapter
            .transaction(tx_hash)
            .await
            .map_err(chain_err)?;
        let receipt = runtime.adapter.receipt(tx_hash).await.map_err(chain_err)?;
        let transfers = runtime
            .adapter
            .transaction_transfers(tx_hash)
            .await
            .map_err(chain_err)?;
        let selected = transfers
            .iter()
            .find(|t| t.to.eq_ignore_ascii_case(&predicted))
            .ok_or_else(|| {
                ToolError::Permanent(
                    "transaction has no transfer to the deterministic FlowPay checkout address"
                        .into(),
                )
            })?;
        let canonical = runtime
            .adapter
            .confirmation_status(receipt.block_number, &receipt.block_hash, 1)
            .await
            .is_ok();
        Ok(VerifiedTransaction {
            chain: chain.clone(),
            hash: tx_hash.to_owned(),
            from: raw.from,
            to: selected.to.clone(),
            token_contract: selected.token_contract.clone(),
            amount: selected.amount.clone(),
            success: receipt.success,
            canonical,
        })
    }

    async fn load_plan(
        &self,
        plan_id: RecoveryPlanId,
    ) -> Result<(RecoveryPlan, String), ToolError> {
        let row = sqlx::query("SELECT canonical_plan,plan_hash FROM recovery_plans WHERE id=$1")
            .bind(plan_id.0)
            .fetch_optional(self.state.store.pool())
            .await
            .map_err(db_err)?
            .ok_or_else(|| ToolError::NotFound("recovery plan".into()))?;
        let value: Value = row.try_get("canonical_plan").map_err(db_err)?;
        let plan: RecoveryPlan =
            serde_json::from_value(value).map_err(|e| ToolError::Permanent(e.to_string()))?;
        let hash: String = row.try_get("plan_hash").map_err(db_err)?;
        Ok((plan, hash))
    }
}

#[async_trait]
impl InvestigationTools for DatabaseAgentTools {
    async fn get_payment(&self, ctx: &AgentContext) -> Result<PaymentSnapshot, ToolError> {
        let p = self
            .state
            .store
            .get_payment_by_id(ctx.payment_id)
            .await
            .map_err(store_err)?;
        Ok(PaymentSnapshot {
            payment_id: p.id,
            expected_chain: p.expected_chain,
            expected_asset: p.expected_asset.symbol,
            expected_amount: p.expected_amount,
            checkout_address: p.checkout_address,
            current_state: p.state.as_str().into(),
        })
    }
    async fn get_claim(&self, ctx: &AgentContext) -> Result<ClaimSnapshot, ToolError> {
        let c = self
            .state
            .store
            .get_claim_by_id(ctx.claim_id)
            .await
            .map_err(store_err)?;
        let chain = c
            .claimed_chain
            .clone()
            .unwrap_or_else(|| c.expected_chain.clone());
        Ok(ClaimSnapshot {
            claim_id: c.id,
            claimed_chain: c.claimed_chain,
            transaction_hash: c.transaction_hash,
            requested_destination: AddressRef {
                chain,
                value: c.recovery_destination,
            },
            explanation: c.explanation,
        })
    }
    async fn verify_wallet_signature(
        &self,
        ctx: &AgentContext,
    ) -> Result<WalletAuthorizationResult, ToolError> {
        let wallet = self
            .state
            .store
            .wallet_authorization(ctx.claim_id)
            .await
            .map_err(store_err)?;
        Ok(WalletAuthorizationResult {
            verified: wallet.is_some(),
            wallet,
            reason: if self
                .state
                .store
                .wallet_authorization(ctx.claim_id)
                .await
                .map_err(store_err)?
                .is_some()
            {
                "verified EIP-191 claim challenge".into()
            } else {
                "no verified self-custody signature".into()
            },
        })
    }
    async fn get_transaction(
        &self,
        ctx: &AgentContext,
        chain: ChainKey,
        transaction_hash: &str,
    ) -> Result<VerifiedTransaction, ToolError> {
        let attempts = self.state.config.agent_retry_budget.clamp(1, 10);
        let mut last_retryable = None;
        for attempt in 1..=attempts {
            match self.verified_transfer(ctx, &chain, transaction_hash).await {
                Ok(tx) => return Ok(tx),
                Err(ToolError::Retryable(message)) => {
                    last_retryable = Some(message);
                    if attempt < attempts {
                        tokio::time::sleep(std::time::Duration::from_millis(
                            200_u64.saturating_mul(attempt as u64),
                        ))
                        .await;
                    }
                }
                Err(error) => return Err(error),
            }
        }
        Err(ToolError::Retryable(format!(
            "chain verification failed after {attempts} bounded attempts: {}",
            last_retryable.unwrap_or_else(|| "provider unavailable".into())
        )))
    }
    async fn verify_counterfactual_address(
        &self,
        ctx: &AgentContext,
        chain: ChainKey,
        candidate_address: &str,
    ) -> Result<CounterfactualVerification, ToolError> {
        let runtime = self
            .state
            .chains
            .get(&chain)
            .ok_or_else(|| ToolError::Permanent("unsupported network".into()))?;
        let payment = self
            .state
            .store
            .get_payment_by_id(ctx.payment_id)
            .await
            .map_err(store_err)?;
        let salt = self
            .state
            .store
            .checkout_salt(payment.id, &payment.expected_chain)
            .await
            .map_err(store_err)?;
        let predicted = runtime.deriver.checkout_hex(salt);
        let factory = runtime
            .adapter
            .verify_factory(&runtime.factory)
            .await
            .map_err(chain_err)?;
        Ok(CounterfactualVerification {
            matches: predicted.eq_ignore_ascii_case(candidate_address),
            predicted_address: predicted,
            factory_verified: factory.recovery_capable,
        })
    }
    async fn get_token_balance(
        &self,
        _ctx: &AgentContext,
        chain: ChainKey,
        token_contract: &str,
        address: &str,
    ) -> Result<AtomicAmount, ToolError> {
        let runtime = self
            .state
            .chains
            .get(&chain)
            .ok_or_else(|| ToolError::Permanent("unsupported network".into()))?;
        runtime
            .adapter
            .token_balance(token_contract, address)
            .await
            .map_err(chain_err)
    }

    async fn build_recovery_plan(&self, ctx: &AgentContext) -> Result<RecoveryPlan, ToolError> {
        let payment = self
            .state
            .store
            .get_payment_by_id(ctx.payment_id)
            .await
            .map_err(store_err)?;
        let claim = self
            .state
            .store
            .get_claim_by_id(ctx.claim_id)
            .await
            .map_err(store_err)?;
        let chain = claim
            .claimed_chain
            .clone()
            .ok_or_else(|| ToolError::InvalidInput("claim missing actual chain".into()))?;
        let tx_hash = claim
            .transaction_hash
            .as_deref()
            .ok_or_else(|| ToolError::InvalidInput("claim missing transaction hash".into()))?;
        let tx = self.verified_transfer(ctx, &chain, tx_hash).await?;
        let token = tx
            .token_contract
            .clone()
            .ok_or_else(|| ToolError::Permanent("native recovery is not enabled".into()))?;
        let runtime = self
            .state
            .chains
            .get(&chain)
            .ok_or_else(|| ToolError::Permanent("unsupported network".into()))?;
        let salt = self
            .state
            .store
            .checkout_salt(payment.id, &payment.expected_chain)
            .await
            .map_err(store_err)?;
        let predicted = runtime.deriver.checkout_hex(salt);
        let factory = runtime
            .adapter
            .verify_factory(&runtime.factory)
            .await
            .map_err(chain_err)?;
        let balance = runtime
            .adapter
            .token_balance(&token, &predicted)
            .await
            .map_err(chain_err)?;
        let ownership = self
            .state
            .store
            .wallet_authorization(claim.id)
            .await
            .map_err(store_err)?
            .is_some();
        let operator_balance = runtime
            .adapter
            .native_balance(&self.state.config.operator_address)
            .await
            .map_err(chain_err)?;
        let minimum_gas = AtomicAmount::from_str("100000000000000")
            .map_err(|e| ToolError::Permanent(e.to_string()))?;
        let gas_sufficient = operator_balance >= minimum_gas;
        let metadata = runtime
            .adapter
            .token_metadata(&token)
            .await
            .map_err(chain_err)?;
        let supported = self
            .state
            .store
            .recovery_asset_by_contract(&chain, &token)
            .await
            .ok();
        let mut risk_flags = Vec::new();
        if chain != payment.expected_chain {
            risk_flags.push(RiskFlag::CrossChain);
        }
        if tx.amount != payment.expected_amount {
            risk_flags.push(RiskFlag::AmountMismatch);
        }
        if balance < tx.amount {
            risk_flags.push(RiskFlag::BalanceChanged);
        }
        if !factory.recovery_capable {
            risk_flags.push(RiskFlag::FactoryMismatch);
        }
        let gross_amount = if balance < tx.amount {
            balance.clone()
        } else {
            tx.amount.clone()
        };
        // Deduct 10% platform fee; owner receives net amount.
        let recover_amount = flowpay_domain::owner_receivable_amount(&gross_amount);
        let supported_assets = supported.map_or_else(Vec::new, |a| {
            vec![SupportedAsset {
                chain: chain.clone(),
                symbol: a.symbol,
                token_contract: a.token_contract,
            }]
        });
        let policy = RecoveryPolicy {
            version: "demo-v1".into(),
            supported_chains: BTreeSet::from([chain.to_string()]),
            supported_assets,
            minimum_recovery_amount: AtomicAmount::from_str("1").unwrap_or_default(),
            maximum_demo_recovery_amount: AtomicAmount::from_decimal("1000", metadata.decimals)
                .map_err(|e| ToolError::Permanent(e.to_string()))?,
            require_self_custody_signature: true,
            require_simulation: true,
            require_human_approval: true,
        };
        let receiver_deployment_required = !runtime
            .adapter
            .has_code(&predicted)
            .await
            .map_err(chain_err)?;
        let facts = RecoveryFacts {
            claim_id: claim.id,
            payment_id: payment.id,
            source_chain: chain.clone(),
            asset_symbol: metadata.symbol,
            token_contract: Some(token),
            amount: recover_amount,
            checkout_address: AddressRef {
                chain: chain.clone(),
                value: predicted,
            },
            recovery_destination: AddressRef {
                chain: chain.clone(),
                value: claim.recovery_destination,
            },
            receiver_deployment_required,
            estimated_gas_atomic: AtomicAmount::from_str("300000").unwrap_or_default(),
            ownership_verified: ownership,
            factory_verified: factory.recovery_capable,
            funds_present: !balance.is_zero(),
            gas_sufficient,
            risk_flags,
        };
        let (hashed, _) = build_plan(&policy, facts);
        let canonical: Value = serde_json::from_str(&hashed.canonical_json)
            .map_err(|e| ToolError::Permanent(e.to_string()))?;
        sqlx::query("INSERT INTO recovery_plans(id,public_id,claim_id,payment_id,source_chain,asset_symbol,token_contract,amount_atomic,checkout_address,recovery_destination,receiver_deployment_required,estimated_gas_atomic,policy_version,policy_decision,simulation_status,risk_flags,required_approval,canonical_plan,plan_hash) VALUES($1,$2,$3,$4,$5,$6,$7,$8::numeric,$9,$10,$11,$12::numeric,$13,$14,$15,$16,$17,$18,$19) ON CONFLICT(plan_hash) DO NOTHING")
            .bind(hashed.plan.id.0).bind(format!("rpl_{}",hashed.plan.id.0.simple())).bind(hashed.plan.claim_id.0).bind(hashed.plan.payment_id.0).bind(chain.to_string()).bind(&hashed.plan.asset_symbol).bind(&hashed.plan.token_contract).bind(hashed.plan.amount.to_string()).bind(&hashed.plan.checkout_address.value).bind(&hashed.plan.recovery_destination.value).bind(hashed.plan.receiver_deployment_required).bind(hashed.plan.estimated_gas_atomic.to_string()).bind(&hashed.plan.policy_version).bind(policy_decision(&hashed.plan.policy_decision)).bind(simulation_status(&hashed.plan.simulation_status)).bind(hashed.plan.risk_flags.iter().map(risk_flag).collect::<Vec<_>>()).bind(hashed.plan.required_approval).bind(canonical).bind(&hashed.plan_hash).execute(self.state.store.pool()).await.map_err(db_err)?;
        Ok(hashed.plan)
    }

    async fn simulate_recovery(
        &self,
        ctx: &AgentContext,
        plan_id: RecoveryPlanId,
    ) -> Result<bool, ToolError> {
        let (plan, hash) = self.load_plan(plan_id).await?;
        if plan.claim_id != ctx.claim_id {
            return Err(ToolError::NotAuthorized);
        }
        if !matches!(
            plan.policy_decision,
            RecoveryPolicyDecision::Allowed | RecoveryPolicyDecision::NeedsFunding
        ) {
            return Ok(false);
        }
        let runtime = self
            .state
            .chains
            .get(&plan.source_chain)
            .ok_or_else(|| ToolError::Permanent("unsupported network".into()))?;
        let salt = self
            .state
            .store
            .checkout_salt(
                plan.payment_id,
                &self
                    .state
                    .store
                    .get_payment_by_id(plan.payment_id)
                    .await
                    .map_err(store_err)?
                    .expected_chain,
            )
            .await
            .map_err(store_err)?;
        let tx = build_live_recover_erc20_transaction(
            &RecoveryPlan {
                simulation_status: SimulationStatus::Succeeded,
                ..plan.clone()
            },
            salt,
            &runtime.factory,
        )
        .map_err(|e| ToolError::Permanent(e.to_string()))?;
        let before_destination = runtime
            .adapter
            .token_balance(
                plan.token_contract
                    .as_deref()
                    .ok_or_else(|| ToolError::Permanent("token missing".into()))?,
                &plan.recovery_destination.value,
            )
            .await
            .map_err(chain_err)?;
        let before_checkout = runtime
            .adapter
            .token_balance(
                plan.token_contract.as_deref().unwrap(),
                &plan.checkout_address.value,
            )
            .await
            .map_err(chain_err)?;
        let simulation = runtime.adapter.simulate(&tx).await.map_err(chain_err)?;
        let hashed = with_simulation(
            HashedRecoveryPlan {
                plan,
                canonical_json: String::new(),
                plan_hash: hash,
            },
            simulation.success,
        );
        let canonical =
            serde_json::to_value(&hashed.plan).map_err(|e| ToolError::Permanent(e.to_string()))?;
        sqlx::query("UPDATE recovery_plans SET simulation_status=$2,simulation_result=$3,canonical_plan=$4,plan_hash=$5,updated_at=now() WHERE id=$1").bind(plan_id.0).bind(simulation_status(&hashed.plan.simulation_status)).bind(json!({"success":simulation.success,"gas_estimate":simulation.gas_estimate,"revert_reason":simulation.revert_reason,"destination_balance_before":before_destination,"checkout_balance_before":before_checkout})).bind(canonical).bind(&hashed.plan_hash).execute(self.state.store.pool()).await.map_err(db_err)?;
        Ok(simulation.success)
    }
}

#[async_trait]
impl ControlledRecoveryTools for DatabaseAgentTools {
    async fn request_approval(
        &self,
        ctx: &AgentContext,
        plan_id: RecoveryPlanId,
    ) -> Result<ApprovalRequestResult, ToolError> {
        let (plan, plan_hash) = self.load_plan(plan_id).await?;
        if plan.claim_id != ctx.claim_id || plan.payment_id != ctx.payment_id {
            return Err(ToolError::NotAuthorized);
        }
        if plan.policy_decision != RecoveryPolicyDecision::Allowed
            || plan.simulation_status != SimulationStatus::Succeeded
        {
            return Err(ToolError::PolicyDenied(
                "policy and successful simulation are required".into(),
            ));
        }
        let claim = self
            .state
            .store
            .get_claim_by_id(ctx.claim_id)
            .await
            .map_err(store_err)?;
        let payment = self
            .state
            .store
            .get_payment_by_id(ctx.payment_id)
            .await
            .map_err(store_err)?;
        if claim
            .claimed_chain
            .as_ref()
            .is_some_and(|c| *c != payment.expected_chain)
            && payment.state == PaymentState::ClaimPending
        {
            self.state
                .store
                .set_payment_state(
                    payment.id,
                    PaymentState::WrongChainClaimed,
                    "wrong_chain_verified",
                    claim.claimed_chain.as_ref(),
                    claim.transaction_hash.as_deref(),
                )
                .await
                .map_err(store_err)?;
            self.state
                .store
                .set_payment_state(
                    payment.id,
                    PaymentState::RecoveryAvailable,
                    "recovery_plan_simulated",
                    claim.claimed_chain.as_ref(),
                    claim.transaction_hash.as_deref(),
                )
                .await
                .map_err(store_err)?;
        } else if payment.state == PaymentState::ClaimPending {
            self.state
                .store
                .set_payment_state(
                    payment.id,
                    PaymentState::RecoveryAvailable,
                    "recovery_plan_simulated",
                    claim.claimed_chain.as_ref(),
                    claim.transaction_hash.as_deref(),
                )
                .await
                .map_err(store_err)?;
        }
        if claim.state == ClaimState::Investigating {
            self.state
                .store
                .set_claim_state(
                    claim.id,
                    ClaimState::Recoverable,
                    "recovery_plan_simulated",
                    "AGENT",
                )
                .await
                .map_err(store_err)?;
        }
        self.state
            .store
            .set_claim_state(
                claim.id,
                ClaimState::ApprovalPending,
                "human_approval_required",
                "AGENT",
            )
            .await
            .map_err(store_err)?;
        let approval = ApprovalId::new();
        let nonce = Uuid::now_v7().simple().to_string();
        sqlx::query("INSERT INTO approvals(id,public_id,claim_id,recovery_plan_id,plan_hash,status,approval_nonce,expires_at) VALUES($1,$2,$3,$4,$5,'PENDING',$6,$7)").bind(approval.0).bind(format!("apr_{}",approval.0.simple())).bind(claim.id.0).bind(plan.id.0).bind(&plan_hash).bind(nonce).bind(OffsetDateTime::now_utc()+Duration::minutes(15)).execute(self.state.store.pool()).await.map_err(db_err)?;
        emit_claim_event(&self.state,claim.merchant_id,"claim.recoverable",&claim.public_id,json!({"claim_id":claim.public_id,"plan_id":format!("rpl_{}",plan.id.0.simple()),"estimated_gas":plan.estimated_gas_atomic,"simulation":"SUCCEEDED","approval_required":true})).await?;
        Ok(ApprovalRequestResult {
            approval_id: approval,
            status: "PENDING".into(),
        })
    }

    async fn execute_proven_recovery(
        &self,
        ctx: &AgentContext,
        plan_id: RecoveryPlanId,
    ) -> Result<RecoveryExecutionResult, ToolError> {
        let (plan, plan_hash) = self.load_plan(plan_id).await?;
        if plan.claim_id != ctx.claim_id || plan.payment_id != ctx.payment_id {
            return Err(ToolError::NotAuthorized);
        }
        if plan.policy_decision != RecoveryPolicyDecision::Allowed
            || plan.simulation_status != SimulationStatus::Succeeded
        {
            return Err(ToolError::PolicyDenied(
                "policy and successful simulation are required".into(),
            ));
        }
        let claim = self
            .state
            .store
            .get_claim_by_id(ctx.claim_id)
            .await
            .map_err(store_err)?;
        let payment = self
            .state
            .store
            .get_payment_by_id(ctx.payment_id)
            .await
            .map_err(store_err)?;
        if claim
            .claimed_chain
            .as_ref()
            .is_some_and(|c| *c != payment.expected_chain)
            && payment.state == PaymentState::ClaimPending
        {
            self.state
                .store
                .set_payment_state(
                    payment.id,
                    PaymentState::WrongChainClaimed,
                    "wrong_chain_verified",
                    claim.claimed_chain.as_ref(),
                    claim.transaction_hash.as_deref(),
                )
                .await
                .map_err(store_err)?;
            self.state
                .store
                .set_payment_state(
                    payment.id,
                    PaymentState::RecoveryAvailable,
                    "recovery_plan_simulated",
                    claim.claimed_chain.as_ref(),
                    claim.transaction_hash.as_deref(),
                )
                .await
                .map_err(store_err)?;
        } else if payment.state == PaymentState::ClaimPending {
            self.state
                .store
                .set_payment_state(
                    payment.id,
                    PaymentState::RecoveryAvailable,
                    "recovery_plan_simulated",
                    claim.claimed_chain.as_ref(),
                    claim.transaction_hash.as_deref(),
                )
                .await
                .map_err(store_err)?;
        }
        // Transition claim through Recoverable → RecoveryPending (skip ApprovalPending)
        if claim.state == ClaimState::Investigating {
            self.state
                .store
                .set_claim_state(
                    claim.id,
                    ClaimState::Recoverable,
                    "recovery_plan_simulated",
                    "AGENT",
                )
                .await
                .map_err(store_err)?;
        }
        self.state
            .store
            .set_claim_state(
                claim.id,
                ClaimState::RecoveryPending,
                "proven_recovery_auto_executed",
                "AGENT",
            )
            .await
            .map_err(store_err)?;
        // Auto-create and immediately consume an approval (no human checkpoint)
        let approval = ApprovalId::new();
        let nonce = Uuid::now_v7().simple().to_string();
        sqlx::query("INSERT INTO approvals(id,public_id,claim_id,recovery_plan_id,plan_hash,status,approval_nonce,expires_at) VALUES($1,$2,$3,$4,$5,'APPROVED',$6,$7)").bind(approval.0).bind(format!("apr_{}",approval.0.simple())).bind(claim.id.0).bind(plan.id.0).bind(&plan_hash).bind(nonce).bind(OffsetDateTime::now_utc()+Duration::minutes(15)).execute(self.state.store.pool()).await.map_err(db_err)?;
        // Now execute using the existing approval path
        self.execute_approved_recovery(ctx, plan_id, approval).await
    }

    async fn execute_approved_recovery(
        &self,
        ctx: &AgentContext,
        plan_id: RecoveryPlanId,
        approval_id: ApprovalId,
    ) -> Result<RecoveryExecutionResult, ToolError> {
        let (plan, plan_hash) = self.load_plan(plan_id).await?;
        if plan.claim_id != ctx.claim_id {
            return Err(ToolError::NotAuthorized);
        }
        let row = sqlx::query(
            "SELECT status,plan_hash,expires_at FROM approvals WHERE id=$1 AND recovery_plan_id=$2",
        )
        .bind(approval_id.0)
        .bind(plan_id.0)
        .fetch_optional(self.state.store.pool())
        .await
        .map_err(db_err)?
        .ok_or_else(|| ToolError::NotFound("approval".into()))?;
        let status: String = row.try_get("status").map_err(db_err)?;
        let approved_hash: String = row.try_get("plan_hash").map_err(db_err)?;
        let expires: OffsetDateTime = row.try_get("expires_at").map_err(db_err)?;
        if status != "APPROVED" || expires < OffsetDateTime::now_utc() {
            return Err(ToolError::NotAuthorized);
        }
        if approved_hash != plan_hash {
            return Err(ToolError::PolicyDenied(
                "approval plan hash no longer matches current plan".into(),
            ));
        }
        let runtime = self
            .state
            .chains
            .get(&plan.source_chain)
            .ok_or_else(|| ToolError::Permanent("unsupported network".into()))?;
        let payment = self
            .state
            .store
            .get_payment_by_id(plan.payment_id)
            .await
            .map_err(store_err)?;
        let salt = self
            .state
            .store
            .checkout_salt(plan.payment_id, &payment.expected_chain)
            .await
            .map_err(store_err)?;
        let tx = build_live_recover_erc20_transaction(&plan, salt, &runtime.factory)
            .map_err(|e| ToolError::Permanent(e.to_string()))?;
        let policy = SignerPolicy {
            allowed_classes: BTreeSet::from([TransactionClass::RecoverErc20]),
            factory_address: runtime.factory.clone(),
        };
        let request = ApprovedSignerRequest {
            plan_id,
            approval_id,
            approved_plan_hash: approved_hash.clone(),
            computed_plan_hash: plan_hash.clone(),
            approval_reserved_for_execution: true,
            transaction_class: TransactionClass::RecoverErc20,
            transaction: tx,
            expected_factory: runtime.factory.clone(),
        };
        let (tx_hash, signer_key_ref) = if let Some(private_key) =
            self.state.config.operator_private_key.as_deref()
        {
            let signer = TestnetKeySigner::from_private_key(policy, &runtime.rpc_url, private_key)
                .map_err(|e| ToolError::Permanent(e.to_string()))?;
            (
                signer
                    .submit_recovery(&request)
                    .await
                    .map_err(|e| ToolError::Permanent(e.to_string()))?,
                "configured-testnet-key",
            )
        } else {
            let signer = DevUnlockedSigner::new(
                policy,
                &runtime.rpc_url,
                &self.state.config.operator_address,
            );
            (
                signer
                    .submit_recovery(&request)
                    .await
                    .map_err(|e| ToolError::Permanent(e.to_string()))?,
                "local-unlocked-operator",
            )
        };
        let mut db_tx = self.state.store.pool().begin().await.map_err(db_err)?;
        let consumed=sqlx::query("UPDATE approvals SET status='CONSUMED',consumed_at=now() WHERE id=$1 AND status='APPROVED'").bind(approval_id.0).execute(&mut *db_tx).await.map_err(db_err)?.rows_affected();
        if consumed != 1 {
            return Err(ToolError::NotAuthorized);
        }
        sqlx::query("INSERT INTO recovery_executions(recovery_plan_id,approval_id,plan_hash,transaction_class,chain,signer_key_ref,tx_hash,state) VALUES($1,$2,$3,'RECOVER_ERC20',$4,$5,$6,'SUBMITTED')").bind(plan_id.0).bind(approval_id.0).bind(&plan_hash).bind(plan.source_chain.to_string()).bind(signer_key_ref).bind(&tx_hash).execute(&mut *db_tx).await.map_err(db_err)?;
        db_tx.commit().await.map_err(db_err)?;
        self.state
            .store
            .set_claim_state(
                ctx.claim_id,
                ClaimState::RecoveryPending,
                "approved_recovery_submitted",
                "AGENT",
            )
            .await
            .map_err(store_err)?;
        self.state
            .store
            .set_payment_state(
                ctx.payment_id,
                PaymentState::RecoveryPending,
                "approved_recovery_submitted",
                Some(&plan.source_chain),
                Some(&tx_hash),
            )
            .await
            .map_err(store_err)?;
        let claim = self
            .state
            .store
            .get_claim_by_id(ctx.claim_id)
            .await
            .map_err(store_err)?;
        emit_claim_event(
            &self.state,
            claim.merchant_id,
            "claim.recovery_pending",
            &claim.public_id,
            json!({"claim_id":claim.public_id,"transaction_hash":tx_hash}),
        )
        .await?;
        Ok(RecoveryExecutionResult {
            transaction_hash: tx_hash,
            submitted: true,
        })
    }

    async fn verify_recovery(
        &self,
        ctx: &AgentContext,
        plan_id: RecoveryPlanId,
        transaction_hash: &str,
    ) -> Result<bool, ToolError> {
        let (plan, _) = self.load_plan(plan_id).await?;
        let runtime = self
            .state
            .chains
            .get(&plan.source_chain)
            .ok_or_else(|| ToolError::Permanent("unsupported network".into()))?;
        let mut receipt_opt = None;
        for _ in 0..15 {
            match runtime.adapter.receipt(transaction_hash).await {
                Ok(r) => {
                    receipt_opt = Some(r);
                    break;
                }
                Err(flowpay_chains::ChainError::TransactionNotFound) => {
                    tokio::time::sleep(std::time::Duration::from_millis(400)).await
                }
                Err(e) => return Err(chain_err(e)),
            }
        }
        let receipt = receipt_opt
            .ok_or_else(|| ToolError::Retryable("recovery receipt not yet available".into()))?;
        if !receipt.success {
            return Ok(false);
        }
        let token = plan
            .token_contract
            .as_deref()
            .ok_or_else(|| ToolError::Permanent("token missing".into()))?;
        let row = sqlx::query("SELECT simulation_result FROM recovery_plans WHERE id=$1")
            .bind(plan_id.0)
            .fetch_one(self.state.store.pool())
            .await
            .map_err(db_err)?;
        let simulation: Value = row.try_get("simulation_result").map_err(db_err)?;
        let before = simulation
            .get("destination_balance_before")
            .and_then(Value::as_str)
            .ok_or_else(|| ToolError::Permanent("simulation missing destination balance".into()))?
            .parse::<AtomicAmount>()
            .map_err(|e| ToolError::Permanent(e.to_string()))?;
        let expected = before.checked_add(&plan.amount);
        let after = runtime
            .adapter
            .token_balance(token, &plan.recovery_destination.value)
            .await
            .map_err(chain_err)?;
        let verified = after >= expected;
        if verified {
            sqlx::query("UPDATE recovery_executions SET state='CONFIRMED',receipt=$2,verified_balance_delta=$3,updated_at=now() WHERE recovery_plan_id=$1").bind(plan_id.0).bind(json!({"transaction_hash":transaction_hash,"block_number":receipt.block_number,"block_hash":receipt.block_hash,"success":true})).bind(json!({"destination_before":before,"destination_after":after,"expected_delta":plan.amount})).execute(self.state.store.pool()).await.map_err(db_err)?;
            self.state
                .store
                .set_claim_state(
                    ctx.claim_id,
                    ClaimState::Recovered,
                    "recovery_receipt_and_balance_verified",
                    "AGENT",
                )
                .await
                .map_err(store_err)?;
            self.state
                .store
                .set_payment_state(
                    ctx.payment_id,
                    PaymentState::Recovered,
                    "recovery_receipt_and_balance_verified",
                    Some(&plan.source_chain),
                    Some(transaction_hash),
                )
                .await
                .map_err(store_err)?;
            let claim = self
                .state
                .store
                .get_claim_by_id(ctx.claim_id)
                .await
                .map_err(store_err)?;
            emit_claim_event(&self.state,claim.merchant_id,"claim.recovered",&claim.public_id,json!({"claim_id":claim.public_id,"transaction_hash":transaction_hash,"amount_atomic":plan.amount,"asset":plan.asset_symbol})).await?;
        }
        Ok(verified)
    }
}

async fn emit_claim_event(
    state: &AppState,
    merchant: flowpay_domain::MerchantId,
    event_type: &str,
    claim_id: &str,
    payload: Value,
) -> Result<(), ToolError> {
    state
        .store
        .enqueue_merchant_webhook_event(merchant, event_type, "CLAIM", claim_id, payload)
        .await
        .map_err(store_err)?;
    Ok(())
}
fn policy_decision(v: &RecoveryPolicyDecision) -> &'static str {
    match v {
        RecoveryPolicyDecision::Allowed => "ALLOWED",
        RecoveryPolicyDecision::Denied => "DENIED",
        RecoveryPolicyDecision::RequiresEscalation => "REQUIRES_ESCALATION",
        RecoveryPolicyDecision::NeedsFunding => "NEEDS_FUNDING",
    }
}
fn simulation_status(v: &SimulationStatus) -> &'static str {
    match v {
        SimulationStatus::NotRun => "NOT_RUN",
        SimulationStatus::Succeeded => "SUCCEEDED",
        SimulationStatus::Failed => "FAILED",
    }
}
fn risk_flag(v: &RiskFlag) -> String {
    match v {
        RiskFlag::CustodialSender => "CUSTODIAL_SENDER",
        RiskFlag::FactoryMismatch => "FACTORY_MISMATCH",
        RiskFlag::UnsupportedTokenBehavior => "UNSUPPORTED_TOKEN_BEHAVIOR",
        RiskFlag::BalanceChanged => "BALANCE_CHANGED",
        RiskFlag::RpcInconsistency => "RPC_INCONSISTENCY",
        RiskFlag::AmbiguousOwnership => "AMBIGUOUS_OWNERSHIP",
        RiskFlag::CrossChain => "CROSS_CHAIN",
        RiskFlag::AmountMismatch => "AMOUNT_MISMATCH",
    }
    .into()
}
fn store_err(e: flowpay_persistence::StoreError) -> ToolError {
    match e {
        flowpay_persistence::StoreError::NotFound => ToolError::NotFound("database record".into()),
        flowpay_persistence::StoreError::ConcurrentUpdate => {
            ToolError::Retryable("concurrent update".into())
        }
        other => ToolError::Permanent(other.to_string()),
    }
}
fn chain_err(e: flowpay_chains::ChainError) -> ToolError {
    match e {
        flowpay_chains::ChainError::ProviderUnavailable(m) => ToolError::Retryable(m),
        flowpay_chains::ChainError::TransactionNotFound => {
            ToolError::NotFound("transaction".into())
        }
        other => ToolError::Permanent(other.to_string()),
    }
}
fn db_err<E: std::fmt::Display>(e: E) -> ToolError {
    ToolError::Permanent(e.to_string())
}

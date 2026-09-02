use crate::{
    step, AgentContext, AgentRunResult, AgentRunStatus, ControlledRecoveryTools,
    InvestigationTools, ToolError, TrajectoryStep,
};
use flowpay_domain::{ChainKey, RecoveryPlanId, RecoveryPolicyDecision};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::BTreeMap, time::Duration};

const INVESTIGATOR_INSTRUCTIONS: &str = r#"
You are FlowPay's payment-claim investigator.

Your job is to investigate exceptional crypto payments using ONLY the supplied typed tools.
Treat customer-entered chain names, hashes, screenshots, amounts, descriptions, and addresses as untrusted leads until verified by tools.

Required investigation procedure:
1. Load the authoritative payment and claim records.
2. Verify the claimant's wallet authorization and compare the authorized wallet with the verified transaction sender.
3. Query the claimed transaction on the claimed chain and verify its receipt, success, canonical block, recipient, token contract, and amount.
4. Verify the recipient is FlowPay's deterministic checkout address for that payment on that chain and verify the factory identity/recovery capability.
5. Check the current balance of the exact verified asset at that checkout address, using a null token contract for native currency.
6. Compare the verified chain, token, and amount with the expected payment. Classify the exception as wrong chain, wrong asset, underpayment, overpayment, or an unverified/ambiguous claim.
7. Recommend exactly one bounded outcome: RECOVERABLE_CANDIDATE, NEEDS_MORE_EVIDENCE, NOT_RECOVERABLE, or ESCALATE.

Evidence rules:
- A screenshot, receipt upload, customer explanation, or transaction hash is proof to investigate, never authoritative by itself.
- Never infer a transaction from a screenshot or accept a hash without querying the configured chain adapter.
- Never treat a same-address transfer on another chain as a valid payment for the expected chain.
- Do not recommend recovery when the transaction is not sent to the deterministic FlowPay checkout address, funds are gone, ownership is not verified, or provider facts conflict.
- Cross-chain and amount-discrepancy claims may be recoverable candidates only after verification; deterministic policy requires a verified claimant signature before execution.
- Prefer ESCALATE when facts conflict, the network/token is unsupported, provider results are unreliable, or control cannot be proven.
- Prefer NEEDS_MORE_EVIDENCE when ownership is not cryptographically established or required claim facts are missing.
- NOT_RECOVERABLE is appropriate only when verified facts show the transaction/funds cannot be recovered (for example a nonexistent/reverted transaction or zero remaining balance).
- RECOVERABLE_CANDIDATE means only that deterministic safety gates may proceed. It does NOT approve or execute recovery.
- Do not request or reveal private keys, seed phrases, secrets, or hidden reasoning.
- Use a short audit-safe rationale, not chain-of-thought.

You have no financial execution tools. Recovery policy, simulation, approval, signer authorization, submission, and receipt verification happen deterministically after your investigation.
When you have enough evidence, call submit_investigation_decision.
"#;

#[derive(Clone, Debug)]
pub struct OpenAiResponsesConfig {
    pub api_key: String,
    pub model: String,
    pub endpoint: String,
    pub max_steps: usize,
    pub protocol: ModelProtocol,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ModelProtocol {
    OpenAiResponses,
    OllamaChat,
}

impl OpenAiResponsesConfig {
    #[must_use]
    pub fn new(api_key: String, model: String) -> Self {
        Self {
            api_key,
            model,
            endpoint: "https://api.openai.com/v1/responses".into(),
            max_steps: 12,
            protocol: ModelProtocol::OpenAiResponses,
        }
    }
}

#[derive(Clone)]
pub struct OpenAiResponsesClient {
    http: reqwest::Client,
    config: OpenAiResponsesConfig,
}

impl OpenAiResponsesClient {
    #[must_use]
    pub fn new(config: OpenAiResponsesConfig) -> Self {
        Self {
            http: reqwest::Client::new(),
            config,
        }
    }

    async fn create_response(&self, input: &[Value]) -> Result<Value, ToolError> {
        if self.config.protocol == ModelProtocol::OllamaChat {
            return self.create_ollama_chat(input).await;
        }
        let body = json!({
            "model": self.config.model,
            "instructions": INVESTIGATOR_INSTRUCTIONS,
            "input": input,
            "tools": tool_schemas(),
            "tool_choice": "required",
            "max_output_tokens": 1200,
            "store": false,
        });
        let response = self
            .http
            .post(&self.config.endpoint)
            .bearer_auth(&self.config.api_key)
            .timeout(Duration::from_secs(120))
            .json(&body)
            .send()
            .await
            .map_err(|e| ToolError::Retryable(format!("model request failed: {e}")))?;
        let status = response.status();
        let payload: Value = response
            .json()
            .await
            .map_err(|e| ToolError::Retryable(format!("model response was not JSON: {e}")))?;
        if status.is_success() {
            return Ok(payload);
        }
        let message = payload
            .pointer("/error/message")
            .and_then(Value::as_str)
            .unwrap_or("unknown model error");
        if status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error() {
            Err(ToolError::Retryable(format!(
                "model HTTP {status}: {message}"
            )))
        } else {
            Err(ToolError::Permanent(format!(
                "model HTTP {status}: {message}"
            )))
        }
    }

    async fn create_ollama_chat(&self, messages: &[Value]) -> Result<Value, ToolError> {
        let tools = tool_schemas()
            .into_iter()
            .map(|schema| {
                json!({
                    "type":"function",
                    "function":{
                        "name":schema.get("name").cloned().unwrap_or(Value::Null),
                        "description":schema.get("description").cloned().unwrap_or(Value::Null),
                        "parameters":schema.get("parameters").cloned().unwrap_or_else(||json!({}))
                    }
                })
            })
            .collect::<Vec<_>>();
        let response = self
            .http
            .post(&self.config.endpoint)
            .timeout(Duration::from_secs(120))
            .json(&json!({
                "model":self.config.model,
                "messages":messages,
                "tools":tools,
                "stream":false,
                "options":{"temperature":0}
            }))
            .send()
            .await
            .map_err(|e| ToolError::Retryable(format!("Ollama request failed: {e}")))?;
        let status = response.status();
        let payload: Value = response
            .json()
            .await
            .map_err(|e| ToolError::Retryable(format!("Ollama response was not JSON: {e}")))?;
        if !status.is_success() {
            let message = payload
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("unknown Ollama error");
            return if status.is_server_error() {
                Err(ToolError::Retryable(format!(
                    "Ollama HTTP {status}: {message}"
                )))
            } else {
                Err(ToolError::Permanent(format!(
                    "Ollama HTTP {status}: {message}"
                )))
            };
        }
        let message = payload.get("message").cloned().ok_or_else(|| {
            ToolError::Permanent("Ollama response contained no assistant message".into())
        })?;
        let mut calls = message
            .get("tool_calls")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        if calls.is_empty() {
            if let Some(content) = message.get("content").and_then(Value::as_str) {
                for call in serde_json::Deserializer::from_str(content)
                    .into_iter::<Value>()
                    .filter_map(Result::ok)
                {
                    if call.get("name").and_then(Value::as_str).is_some() {
                        calls.push(json!({"function":call}));
                    }
                }
            }
        }
        let output = calls
            .iter()
            .enumerate()
            .filter_map(|(index, call)| {
                let function = call.get("function")?;
                let name = function.get("name")?.as_str()?;
                let arguments = function.get("arguments").cloned().unwrap_or_else(||json!({}));
                Some(json!({
                    "type":"function_call",
                    "call_id":format!("ollama-{index}"),
                    "name":name,
                    "arguments":if arguments.is_string(){arguments}else{Value::String(arguments.to_string())}
                }))
            })
            .collect::<Vec<_>>();
        Ok(json!({"output":output,"ollama_message":message}))
    }
}

#[derive(Clone, Debug, Default)]
struct EvidenceLedger {
    payment_loaded: bool,
    claim_loaded: bool,
    claimed_chain: Option<ChainKey>,
    claimed_transaction_hash: Option<String>,
    checkout_address: Option<String>,
    wallet_verified: Option<bool>,
    wallet: Option<String>,
    tx_found: Option<bool>,
    tx_success: Option<bool>,
    tx_canonical: Option<bool>,
    tx_from: Option<String>,
    tx_to: Option<String>,
    tx_token: Option<String>,
    tx_chain: Option<ChainKey>,
    cf_matches: Option<bool>,
    factory_verified: Option<bool>,
    balance_positive: Option<bool>,
    last_errors: BTreeMap<String, String>,
}

impl EvidenceLedger {
    fn recovery_candidate_ready(&self) -> Result<(), String> {
        if !self.payment_loaded || !self.claim_loaded {
            return Err("payment and claim records must be loaded".into());
        }
        if self.wallet_verified != Some(true) {
            return Err("wallet ownership must be cryptographically verified".into());
        }
        if self.tx_found != Some(true)
            || self.tx_success != Some(true)
            || self.tx_canonical != Some(true)
        {
            return Err("transaction must be found, successful, and canonical".into());
        }
        if let (Some(wallet), Some(sender)) = (&self.wallet, &self.tx_from) {
            if !wallet.eq_ignore_ascii_case(sender) {
                return Err("verified wallet does not match transaction sender".into());
            }
        } else {
            return Err("verified wallet and transaction sender are required".into());
        }
        if self.cf_matches != Some(true) || self.factory_verified != Some(true) {
            return Err(
                "counterfactual checkout address and factory control must be verified".into(),
            );
        }
        if self.balance_positive != Some(true) {
            return Err("current asset balance must be positive".into());
        }
        Ok(())
    }

    fn not_recoverable_proven(&self) -> bool {
        self.tx_found == Some(false)
            || self.tx_success == Some(false)
            || self.tx_canonical == Some(false)
            || self.balance_positive == Some(false)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum ModelDisposition {
    RecoverableCandidate,
    NotRecoverable,
    NeedsMoreEvidence,
    Escalate,
}

#[derive(Clone, Debug, Deserialize)]
struct DecisionArgs {
    disposition: ModelDisposition,
    concise_rationale: String,
}

#[derive(Clone, Debug)]
struct FunctionCall {
    call_id: String,
    name: String,
    arguments: String,
}

pub struct ModelDrivenAgent<T> {
    tools: T,
    model: OpenAiResponsesClient,
}

impl<T> ModelDrivenAgent<T>
where
    T: InvestigationTools + ControlledRecoveryTools,
{
    #[must_use]
    pub fn new(tools: T, model: OpenAiResponsesClient) -> Self {
        Self { tools, model }
    }

    pub async fn investigate(&self, ctx: &AgentContext) -> Result<AgentRunResult, ToolError> {
        let prompt = format!(
            "Investigate FlowPay claim {} for payment {}. Select tools yourself. Do not assume the claim is true.",
            ctx.claim_id.0, ctx.payment_id.0
        );
        let mut history = if self.model.config.protocol == ModelProtocol::OllamaChat {
            vec![
                json!({"role":"system","content":INVESTIGATOR_INSTRUCTIONS}),
                json!({"role":"user","content":prompt}),
            ]
        } else {
            vec![json!({"role":"user","content":[{"type":"input_text","text":prompt}]})]
        };
        let mut ledger = EvidenceLedger::default();
        let mut trajectory = Vec::<TrajectoryStep>::new();
        let mut sequence = 1_u32;

        for _ in 0..self.model.config.max_steps {
            let response = self.model.create_response(&history).await?;
            let output = response
                .get("output")
                .and_then(Value::as_array)
                .ok_or_else(|| {
                    ToolError::Permanent("model response contained no output array".into())
                })?;
            let calls = function_calls(output)?;
            if calls.is_empty() {
                return Ok(escalated(
                    "model returned no typed tool call; safe fallback is escalation",
                    trajectory,
                ));
            }

            if self.model.config.protocol == ModelProtocol::OllamaChat {
                history.push(response.get("ollama_message").cloned().unwrap_or_else(|| {
                    json!({
                        "role":"assistant","content":""
                    })
                }));
            } else {
                history.extend(output.iter().cloned());
            }

            for call in calls {
                let (tool_output, verification, rationale, decision) =
                    self.execute_model_tool(ctx, &call, &mut ledger).await;
                let status = if tool_output.get("ok").and_then(Value::as_bool) == Some(false) {
                    "tool returned a structured error"
                } else {
                    "typed tool output recorded"
                };
                trajectory.push(step(
                    sequence,
                    &call.name,
                    redact_input(&call.name, &call.arguments),
                    tool_output.clone(),
                    &format!("{verification}; {status}"),
                    &rationale,
                ));
                sequence = sequence.saturating_add(1);
                let serialized =
                    serde_json::to_string(&tool_output).unwrap_or_else(|_| "{\"ok\":false}".into());
                if self.model.config.protocol == ModelProtocol::OllamaChat {
                    history.push(json!({"role":"tool","tool_name":call.name,"content":serialized}));
                } else {
                    history.push(json!({"type":"function_call_output","call_id":call.call_id,"output":serialized}));
                }

                if let Some(args) = decision {
                    match self.accept_decision(&ledger, &args) {
                        Ok(ModelDisposition::RecoverableCandidate) => {
                            return self
                                .finish_recoverable_candidate(
                                    ctx,
                                    args.concise_rationale,
                                    trajectory,
                                    sequence,
                                )
                                .await;
                        }
                        Ok(ModelDisposition::NotRecoverable) => {
                            return Ok(AgentRunResult {
                                status: AgentRunStatus::NotRecoverable,
                                concise_rationale: args.concise_rationale,
                                plan_id: None,
                                approval_id: None,
                                recovery_transaction_hash: None,
                                trajectory,
                            });
                        }
                        Ok(ModelDisposition::NeedsMoreEvidence) => {
                            return Ok(AgentRunResult {
                                status: AgentRunStatus::NeedsMoreEvidence,
                                concise_rationale: args.concise_rationale,
                                plan_id: None,
                                approval_id: None,
                                recovery_transaction_hash: None,
                                trajectory,
                            });
                        }
                        Ok(ModelDisposition::Escalate) => {
                            return Ok(escalated(&args.concise_rationale, trajectory));
                        }
                        Err(reason) => {
                            // Return deterministic rejection to the model so it can gather the missing fact.
                            let message = format!(
                                "Deterministic decision gate rejected that disposition: {reason}. Use the available investigation tools to resolve it or choose a safe outcome."
                            );
                            if self.model.config.protocol == ModelProtocol::OllamaChat {
                                history.push(json!({"role":"user","content":message}));
                            } else {
                                history.push(json!({
                                    "role":"user",
                                    "content":[{"type":"input_text","text":message}]
                                }));
                            }
                        }
                    }
                }
            }
        }

        Ok(escalated(
            "model exceeded the bounded investigation step limit",
            trajectory,
        ))
    }

    async fn execute_model_tool(
        &self,
        ctx: &AgentContext,
        call: &FunctionCall,
        ledger: &mut EvidenceLedger,
    ) -> (Value, String, String, Option<DecisionArgs>) {
        match call.name.as_str() {
            "get_payment" => match self.tools.get_payment(ctx).await {
                Ok(payment) => {
                    ledger.payment_loaded = true;
                    ledger.checkout_address = Some(payment.checkout_address.value.clone());
                    (
                        json!({"ok":true,"payment":payment}),
                        "authoritative payment record loaded".into(),
                        "The model selected the payment record to establish immutable expected facts.".into(),
                        None,
                    )
                }
                Err(error) => tool_error("get_payment", error, ledger),
            },
            "get_claim" => match self.tools.get_claim(ctx).await {
                Ok(claim) => {
                    ledger.claim_loaded = true;
                    ledger.claimed_chain.clone_from(&claim.claimed_chain);
                    ledger
                        .claimed_transaction_hash
                        .clone_from(&claim.transaction_hash);
                    (
                        json!({"ok":true,"claim":claim}),
                        "claim treated as untrusted investigation input".into(),
                        "The model selected the claim record to learn what must be independently checked.".into(),
                        None,
                    )
                }
                Err(error) => tool_error("get_claim", error, ledger),
            },
            "verify_wallet_signature" => match self.tools.verify_wallet_signature(ctx).await {
                Ok(auth) => {
                    ledger.wallet_verified = Some(auth.verified);
                    ledger.wallet.clone_from(&auth.wallet);
                    (
                        json!({"ok":true,"authorization":auth}),
                        "cryptographic self-custody authorization checked".into(),
                        "The model selected ownership verification before recommending any recovery.".into(),
                        None,
                    )
                }
                Err(error) => tool_error("verify_wallet_signature", error, ledger),
            },
            "get_transaction" => {
                let parsed: Result<GetTransactionArgs, _> = serde_json::from_str(&call.arguments);
                let Ok(_args) = parsed else {
                    return bad_args("get_transaction", ledger);
                };
                let Some(chain) = ledger.claimed_chain.clone() else {
                    return structured_failure(
                        "claim_facts_required",
                        "load the claim before querying its transaction",
                    );
                };
                let Some(transaction_hash) = ledger.claimed_transaction_hash.clone() else {
                    return structured_failure(
                        "claim_facts_required",
                        "the claim has no transaction hash",
                    );
                };
                match self
                    .tools
                    .get_transaction(ctx, chain.clone(), &transaction_hash)
                    .await
                {
                    Ok(tx) => {
                        ledger.tx_found = Some(true);
                        ledger.tx_success = Some(tx.success);
                        ledger.tx_canonical = Some(tx.canonical);
                        ledger.tx_from = Some(tx.from.clone());
                        ledger.tx_to = Some(tx.to.clone());
                        ledger.tx_token.clone_from(&tx.token_contract);
                        ledger.tx_chain = Some(tx.chain.clone());
                        (
                            json!({"ok":true,"transaction":tx}),
                            "transaction independently queried from configured chain adapter".into(),
                            "The model selected on-chain transaction verification instead of trusting the submitted hash.".into(),
                            None,
                        )
                    }
                    Err(ToolError::NotFound(message)) => {
                        ledger.tx_found = Some(false);
                        ledger
                            .last_errors
                            .insert("get_transaction".into(), message.clone());
                        structured_failure("transaction_not_found", &message)
                    }
                    Err(error) => tool_error("get_transaction", error, ledger),
                }
            }
            "verify_counterfactual_address" => {
                let parsed: Result<VerifyAddressArgs, _> = serde_json::from_str(&call.arguments);
                let Ok(_args) = parsed else {
                    return bad_args("verify_counterfactual_address", ledger);
                };
                let Some(chain) = ledger.tx_chain.clone() else {
                    return structured_failure(
                        "transaction_facts_required",
                        "verify the transaction before checking the counterfactual address",
                    );
                };
                let Some(candidate_address) = ledger.tx_to.clone() else {
                    return structured_failure(
                        "transaction_facts_required",
                        "verified transaction has no destination address",
                    );
                };
                match self
                    .tools
                    .verify_counterfactual_address(ctx, chain, &candidate_address)
                    .await
                {
                    Ok(result) => {
                        ledger.cf_matches = Some(result.matches);
                        ledger.factory_verified = Some(result.factory_verified);
                        (
                            json!({"ok":true,"counterfactual":result}),
                            "factory identity and CREATE3 address relationship independently verified".into(),
                            "The model selected deterministic address-control verification before considering recovery.".into(),
                            None,
                        )
                    }
                    Err(error) => tool_error("verify_counterfactual_address", error, ledger),
                }
            }
            "get_asset_balance" => {
                let parsed: Result<GetBalanceArgs, _> = serde_json::from_str(&call.arguments);
                let Ok(_args) = parsed else {
                    return bad_args("get_asset_balance", ledger);
                };
                let Some(chain) = ledger.tx_chain.clone() else {
                    return structured_failure(
                        "transaction_facts_required",
                        "verify the transaction before checking its token balance",
                    );
                };
                let Some(address) = ledger.tx_to.clone() else {
                    return structured_failure(
                        "transaction_facts_required",
                        "verified transaction has no destination address",
                    );
                };
                match self
                    .tools
                    .get_asset_balance(ctx, chain, ledger.tx_token.as_deref(), &address)
                    .await
                {
                    Ok(balance) => {
                        ledger.balance_positive = Some(!balance.is_zero());
                        (
                            json!({"ok":true,"balance_atomic":balance}),
                            "current token balance independently checked on-chain".into(),
                            "The model selected a current balance check because historical receipt evidence does not prove funds remain recoverable.".into(),
                            None,
                        )
                    }
                    Err(error) => tool_error("get_asset_balance", error, ledger),
                }
            }
            "submit_investigation_decision" => {
                let parsed: Result<DecisionArgs, _> = serde_json::from_str(&call.arguments);
                let Ok(args) = parsed else {
                    return bad_args("submit_investigation_decision", ledger);
                };
                (
                    json!({"ok":true,"submitted":args.disposition,"note":"deterministic decision gate will validate this recommendation"}),
                    "model recommendation is advisory until deterministic evidence gates accept it"
                        .into(),
                    args.concise_rationale.clone(),
                    Some(args),
                )
            }
            other => {
                structured_failure("unknown_tool", &format!("tool {other} is not allowlisted"))
            }
        }
    }

    fn accept_decision(
        &self,
        ledger: &EvidenceLedger,
        args: &DecisionArgs,
    ) -> Result<ModelDisposition, String> {
        if args.concise_rationale.trim().len() < 8 || args.concise_rationale.len() > 400 {
            return Err("audit rationale must be concise and between 8 and 400 characters".into());
        }
        match args.disposition {
            ModelDisposition::RecoverableCandidate => {
                ledger.recovery_candidate_ready()?;
                Ok(ModelDisposition::RecoverableCandidate)
            }
            ModelDisposition::NotRecoverable => {
                if ledger.not_recoverable_proven() {
                    Ok(ModelDisposition::NotRecoverable)
                } else {
                    Err("NOT_RECOVERABLE requires a verified terminal fact such as missing/reverted transaction or zero balance".into())
                }
            }
            ModelDisposition::NeedsMoreEvidence => Ok(ModelDisposition::NeedsMoreEvidence),
            ModelDisposition::Escalate => Ok(ModelDisposition::Escalate),
        }
    }

    async fn finish_recoverable_candidate(
        &self,
        ctx: &AgentContext,
        model_rationale: String,
        mut trajectory: Vec<TrajectoryStep>,
        mut sequence: u32,
    ) -> Result<AgentRunResult, ToolError> {
        // From this point onward, no model choice controls financial policy or execution.
        let plan = self.tools.build_recovery_plan(ctx).await?;
        trajectory.push(step(
            sequence,
            "policy_gate.build_recovery_plan",
            json!({"claim_id":ctx.claim_id}),
            json!({"plan_id":plan.id,"policy_decision":plan.policy_decision,"risk_flags":plan.risk_flags,"required_approval":plan.required_approval}),
            "deterministic RecoveryPolicy produced a canonical hashed plan",
            "The model can recommend investigation disposition, but policy determines whether recovery is permitted.",
        ));
        sequence = sequence.saturating_add(1);
        match plan.policy_decision {
            RecoveryPolicyDecision::Denied => {
                return Ok(AgentRunResult {
                    status: AgentRunStatus::NotRecoverable,
                    concise_rationale: "model investigation found a candidate, but deterministic recovery policy denied it".into(),
                    plan_id: Some(plan.id),
                    approval_id: None,
                    recovery_transaction_hash: None,
                    trajectory,
                });
            }
            RecoveryPolicyDecision::RequiresEscalation => {
                return Ok(AgentRunResult {
                    status: AgentRunStatus::Escalated,
                    concise_rationale: "model investigation found a candidate, but deterministic policy requires escalation".into(),
                    plan_id: Some(plan.id),
                    approval_id: None,
                    recovery_transaction_hash: None,
                    trajectory,
                });
            }
            RecoveryPolicyDecision::Allowed | RecoveryPolicyDecision::NeedsFunding => {}
        }

        let simulated = self.tools.simulate_recovery(ctx, plan.id).await?;
        trajectory.push(step(
            sequence,
            "policy_gate.simulate_recovery",
            json!({"plan_id":plan.id}),
            json!({"success":simulated}),
            "simulation is a mandatory deterministic gate",
            "No approval can be requested unless the exact plan simulates successfully.",
        ));
        sequence = sequence.saturating_add(1);
        if !simulated {
            return Ok(AgentRunResult {
                status: AgentRunStatus::Escalated,
                concise_rationale: "recovery candidate failed deterministic transaction simulation"
                    .into(),
                plan_id: Some(plan.id),
                approval_id: None,
                recovery_transaction_hash: None,
                trajectory,
            });
        }
        if plan.policy_decision == RecoveryPolicyDecision::NeedsFunding {
            return Ok(AgentRunResult {
                status: AgentRunStatus::RecoverableAwaitingFunding,
                concise_rationale: "investigation and simulation passed, but the restricted signer requires test gas".into(),
                plan_id: Some(plan.id),
                approval_id: None,
                recovery_transaction_hash: None,
                trajectory,
            });
        }
        if plan.required_approval {
            let approval = self.tools.request_approval(ctx, plan.id).await?;
            trajectory.push(step(
                sequence,
                "policy_gate.request_approval",
                json!({"plan_id":plan.id}),
                json!({"approval_id":approval.approval_id,"status":approval.status}),
                "the verified claimant signature authorizes deterministic execution",
                "The model cannot approve or execute a recovery.",
            ));
            return Ok(AgentRunResult {
                status: AgentRunStatus::RecoverableAwaitingApproval,
                concise_rationale:
                    "recovery passed verification and simulation and has claimant authorization"
                        .into(),
                plan_id: Some(plan.id),
                approval_id: Some(approval.approval_id),
                recovery_transaction_hash: None,
                trajectory,
            });
        }

        // Proven low-risk recovery: auto-execute only after all deterministic gates pass.
        // 10% platform fee is deducted; net amount returned to owner.
        let execution = self.tools.execute_proven_recovery(ctx, plan.id).await?;
        trajectory.push(step(
            sequence,
            "policy_gate.execute_proven_recovery",
            json!({"plan_id":plan.id}),
            json!({"submitted":execution.submitted,"transaction_hash":execution.transaction_hash}),
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
                trajectory,
            });
        }
        sequence = sequence.saturating_add(1);
        let verified = self
            .tools
            .verify_recovery(ctx, plan.id, &execution.transaction_hash)
            .await?;
        trajectory.push(step(
            sequence,
            "policy_gate.verify_recovery",
            json!({"transaction_hash":execution.transaction_hash}),
            json!({"verified":verified}),
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
                format!(
                    "{} Proven recovery auto-executed and verified. 10%% fee deducted.",
                    model_rationale.trim()
                )
            } else {
                "recovery could not be independently verified".into()
            },
            plan_id: Some(plan.id),
            approval_id: None,
            recovery_transaction_hash: Some(execution.transaction_hash),
            trajectory,
        })
    }

    pub async fn execute_after_approval(
        &self,
        ctx: &AgentContext,
        plan_id: RecoveryPlanId,
        approval_id: flowpay_domain::ApprovalId,
    ) -> Result<AgentRunResult, ToolError> {
        // Resumption is intentionally deterministic: the model is not in the money-moving path.
        let execution = self
            .tools
            .execute_approved_recovery(ctx, plan_id, approval_id)
            .await?;
        let mut trajectory = vec![step(
            1,
            "execute_approved_recovery",
            json!({"plan_id":plan_id,"approval_id":approval_id}),
            json!({"submitted":execution.submitted,"transaction_hash":execution.transaction_hash}),
            "restricted signer requires an atomically reserved approval, matching plan hash, and allowlisted transaction class",
            "No model is consulted after approval because financial execution is deterministic.",
        )];
        if !execution.submitted {
            return Ok(escalated("approved recovery was not submitted", trajectory));
        }
        let verified = self
            .tools
            .verify_recovery(ctx, plan_id, &execution.transaction_hash)
            .await?;
        trajectory.push(step(
            2,
            "verify_recovery",
            json!({"transaction_hash":execution.transaction_hash}),
            json!({"verified":verified}),
            "receipt and resulting balance delta independently verified",
            "Submission alone is not considered successful recovery.",
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
                "recovery submission could not be independently verified".into()
            },
            plan_id: Some(plan_id),
            approval_id: Some(approval_id),
            recovery_transaction_hash: Some(execution.transaction_hash),
            trajectory,
        })
    }
}

#[allow(dead_code)]
#[derive(Deserialize)]
struct GetTransactionArgs {
    chain: String,
    transaction_hash: String,
}

#[allow(dead_code)]
#[derive(Deserialize)]
struct VerifyAddressArgs {
    chain: String,
    candidate_address: String,
}

#[allow(dead_code)]
#[derive(Deserialize)]
struct GetBalanceArgs {
    chain: String,
    token_contract: Option<String>,
    address: String,
}

fn function_calls(output: &[Value]) -> Result<Vec<FunctionCall>, ToolError> {
    output
        .iter()
        .filter(|item| item.get("type").and_then(Value::as_str) == Some("function_call"))
        .map(|item| {
            Ok(FunctionCall {
                call_id: item
                    .get("call_id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        ToolError::Permanent("model function call missing call_id".into())
                    })?
                    .to_owned(),
                name: item
                    .get("name")
                    .and_then(Value::as_str)
                    .ok_or_else(|| ToolError::Permanent("model function call missing name".into()))?
                    .to_owned(),
                arguments: item
                    .get("arguments")
                    .and_then(Value::as_str)
                    .unwrap_or("{}")
                    .to_owned(),
            })
        })
        .collect()
}

fn tool_error(
    tool: &str,
    error: ToolError,
    ledger: &mut EvidenceLedger,
) -> (Value, String, String, Option<DecisionArgs>) {
    let (class, message) = match &error {
        ToolError::NotAuthorized => ("authorization_denied", error.to_string()),
        ToolError::InvalidInput(_) => ("invalid_input", error.to_string()),
        ToolError::NotFound(_) => ("not_found", error.to_string()),
        ToolError::PolicyDenied(_) => ("policy_denied", error.to_string()),
        ToolError::Retryable(_) => ("retryable", error.to_string()),
        ToolError::Permanent(_) => ("permanent", error.to_string()),
    };
    ledger.last_errors.insert(tool.into(), message.clone());
    (
        json!({"ok":false,"error":{"class":class,"message":message}}),
        "tool failure was returned as structured evidence; no blockchain fact was invented".into(),
        "The selected tool failed, so the model must retry, request evidence, or escalate rather than guess.".into(),
        None,
    )
}

fn structured_failure(class: &str, message: &str) -> (Value, String, String, Option<DecisionArgs>) {
    (
        json!({"ok":false,"error":{"class":class,"message":message}}),
        "structured tool failure recorded".into(),
        "The investigator must use a safe outcome instead of inventing missing facts.".into(),
        None,
    )
}

fn bad_args(
    tool: &str,
    ledger: &mut EvidenceLedger,
) -> (Value, String, String, Option<DecisionArgs>) {
    ledger
        .last_errors
        .insert(tool.into(), "model supplied invalid typed arguments".into());
    structured_failure(
        "invalid_arguments",
        "tool arguments failed schema validation",
    )
}

fn redact_input(tool: &str, arguments: &str) -> Value {
    let parsed: Value = serde_json::from_str(arguments).unwrap_or_else(|_| json!({}));
    if tool == "submit_investigation_decision" {
        json!({
            "disposition": parsed.get("disposition").cloned().unwrap_or(Value::Null),
            "concise_rationale": parsed.get("concise_rationale").cloned().unwrap_or(Value::Null),
        })
    } else {
        parsed
    }
}

fn escalated(rationale: &str, trajectory: Vec<TrajectoryStep>) -> AgentRunResult {
    AgentRunResult {
        status: AgentRunStatus::Escalated,
        concise_rationale: rationale.into(),
        plan_id: None,
        approval_id: None,
        recovery_transaction_hash: None,
        trajectory,
    }
}

fn tool_schemas() -> Vec<Value> {
    vec![
        empty_tool(
            "get_payment",
            "Load the authoritative expected payment record.",
        ),
        empty_tool(
            "get_claim",
            "Load the customer claim as untrusted investigation input.",
        ),
        empty_tool(
            "verify_wallet_signature",
            "Verify the stored EIP-191 self-custody claim authorization.",
        ),
        json!({
            "type":"function","name":"get_transaction","description":"Independently query and verify the claimed blockchain transaction.","strict":true,
            "parameters":{"type":"object","properties":{"chain":{"type":"string","enum":["base","bsc","bsc_testnet","ethereum_sepolia","base_sepolia","arbitrum_sepolia","optimism_sepolia","polygon_amoy"]},"transaction_hash":{"type":"string"}},"required":["chain","transaction_hash"],"additionalProperties":false}
        }),
        json!({
            "type":"function","name":"verify_counterfactual_address","description":"Verify that a candidate address equals FlowPay's deterministic checkout address on the claimed EVM chain and that the expected factory is recovery-capable.","strict":true,
            "parameters":{"type":"object","properties":{"chain":{"type":"string","enum":["base","bsc","bsc_testnet","ethereum_sepolia","base_sepolia","arbitrum_sepolia","optimism_sepolia","polygon_amoy"]},"candidate_address":{"type":"string"}},"required":["chain","candidate_address"],"additionalProperties":false}
        }),
        json!({
            "type":"function","name":"get_asset_balance","description":"Read the current native or ERC-20 balance at a checkout address on a supported EVM chain. Use null token_contract for native currency.","strict":true,
            "parameters":{"type":"object","properties":{"chain":{"type":"string","enum":["base","bsc","bsc_testnet","ethereum_sepolia","base_sepolia","arbitrum_sepolia","optimism_sepolia","polygon_amoy"]},"token_contract":{"type":["string","null"]},"address":{"type":"string"}},"required":["chain","token_contract","address"],"additionalProperties":false}
        }),
        json!({
            "type":"function","name":"submit_investigation_decision","description":"Submit a constrained investigation recommendation. This cannot approve, sign, or execute recovery.","strict":true,
            "parameters":{"type":"object","properties":{"disposition":{"type":"string","enum":["RECOVERABLE_CANDIDATE","NOT_RECOVERABLE","NEEDS_MORE_EVIDENCE","ESCALATE"]},"concise_rationale":{"type":"string","minLength":8,"maxLength":400}},"required":["disposition","concise_rationale"],"additionalProperties":false}
        }),
    ]
}

fn empty_tool(name: &str, description: &str) -> Value {
    json!({
        "type":"function",
        "name":name,
        "description":description,
        "strict":true,
        "parameters":{"type":"object","properties":{},"required":[],"additionalProperties":false}
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ready_ledger() -> EvidenceLedger {
        EvidenceLedger {
            payment_loaded: true,
            claim_loaded: true,
            wallet_verified: Some(true),
            wallet: Some("0x1111111111111111111111111111111111111111".into()),
            tx_found: Some(true),
            tx_success: Some(true),
            tx_canonical: Some(true),
            tx_from: Some("0x1111111111111111111111111111111111111111".into()),
            tx_to: Some("0x2222222222222222222222222222222222222222".into()),
            tx_token: Some("0x3333333333333333333333333333333333333333".into()),
            tx_chain: Some(ChainKey::Base),
            cf_matches: Some(true),
            factory_verified: Some(true),
            balance_positive: Some(true),
            last_errors: BTreeMap::new(),
            ..EvidenceLedger::default()
        }
    }

    #[test]
    fn recoverable_candidate_requires_all_deterministic_evidence() {
        let ledger = ready_ledger();
        assert!(ledger.recovery_candidate_ready().is_ok());

        let mut missing_auth = ledger.clone();
        missing_auth.wallet_verified = Some(false);
        assert!(missing_auth.recovery_candidate_ready().is_err());

        let mut factory_mismatch = ledger.clone();
        factory_mismatch.factory_verified = Some(false);
        assert!(factory_mismatch.recovery_candidate_ready().is_err());

        let mut empty_balance = ledger;
        empty_balance.balance_positive = Some(false);
        assert!(empty_balance.recovery_candidate_ready().is_err());
    }

    #[test]
    fn wallet_sender_mismatch_can_never_be_a_recoverable_candidate() {
        let mut ledger = ready_ledger();
        ledger.tx_from = Some("0x9999999999999999999999999999999999999999".into());
        assert!(ledger
            .recovery_candidate_ready()
            .unwrap_err()
            .contains("does not match transaction sender"));
    }

    #[test]
    fn not_recoverable_requires_verified_terminal_fact() {
        let mut ledger = ready_ledger();
        assert!(!ledger.not_recoverable_proven());
        ledger.balance_positive = Some(false);
        assert!(ledger.not_recoverable_proven());
    }

    #[test]
    fn model_toolset_contains_no_financial_execution_primitive() {
        let names = tool_schemas()
            .into_iter()
            .filter_map(|tool| tool.get("name").and_then(Value::as_str).map(str::to_owned))
            .collect::<Vec<_>>();
        for forbidden in [
            "sign",
            "send_transaction",
            "approve",
            "execute_recovery",
            "arbitrary_rpc",
            "build_transaction",
        ] {
            assert!(
                !names.iter().any(|name| name == forbidden),
                "forbidden model tool exposed: {forbidden}"
            );
        }
        assert!(names
            .iter()
            .any(|name| name == "submit_investigation_decision"));
    }
}

use async_trait::async_trait;
use ethers_signers::{LocalWallet, Signer};
use flowpay_chains::PreparedTransaction;
use flowpay_domain::{ApprovalId, RecoveryPlanId};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use thiserror::Error;

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TransactionClass {
    DeployCheckout,
    RecoverErc20,
    RecoverNative,
    SettleErc20,
    SettleNative,
}

impl TransactionClass {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::DeployCheckout => "DEPLOY_CHECKOUT",
            Self::RecoverErc20 => "RECOVER_ERC20",
            Self::RecoverNative => "RECOVER_NATIVE",
            Self::SettleErc20 => "SETTLE_ERC20",
            Self::SettleNative => "SETTLE_NATIVE",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ApprovedSignerRequest {
    pub plan_id: RecoveryPlanId,
    pub approval_id: ApprovalId,
    pub approved_plan_hash: String,
    pub computed_plan_hash: String,
    pub approval_reserved_for_execution: bool,
    pub transaction_class: TransactionClass,
    pub transaction: PreparedTransaction,
    pub expected_factory: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SettlementSignerRequest {
    pub payment_id: flowpay_domain::PaymentId,
    pub transaction_class: TransactionClass,
    pub transaction: PreparedTransaction,
    pub expected_factory: String,
    pub settlement_destination: String,
    pub configured_merchant_destination: String,
}

#[derive(Debug, Error)]
pub enum SignerError {
    #[error("transaction class is not allowlisted")]
    ClassDenied,
    #[error("approved plan hash mismatch")]
    PlanHashMismatch,
    #[error("approval was not atomically reserved for this execution")]
    ApprovalNotReserved,
    #[error("transaction target is not the configured factory")]
    WrongTarget,
    #[error("transaction carries native value when class forbids it")]
    UnexpectedValue,
    #[error("settlement destination does not match merchant configuration")]
    WrongSettlementDestination,
    #[error("rpc error: {0}")]
    Rpc(String),
}

#[derive(Clone)]
pub struct SignerPolicy {
    pub allowed_classes: BTreeSet<TransactionClass>,
    pub factory_address: String,
}

impl SignerPolicy {
    pub fn validate(&self, request: &ApprovedSignerRequest) -> Result<(), SignerError> {
        if !self.allowed_classes.contains(&request.transaction_class) {
            return Err(SignerError::ClassDenied);
        }
        if request.approved_plan_hash != request.computed_plan_hash {
            return Err(SignerError::PlanHashMismatch);
        }
        if !request.approval_reserved_for_execution {
            return Err(SignerError::ApprovalNotReserved);
        }
        if !request
            .transaction
            .to
            .eq_ignore_ascii_case(&self.factory_address)
            || !request
                .expected_factory
                .eq_ignore_ascii_case(&self.factory_address)
        {
            return Err(SignerError::WrongTarget);
        }
        if matches!(
            request.transaction_class,
            TransactionClass::DeployCheckout
                | TransactionClass::RecoverErc20
                | TransactionClass::SettleErc20
        ) && !request.transaction.value.is_zero()
        {
            return Err(SignerError::UnexpectedValue);
        }
        Ok(())
    }

    pub fn validate_settlement(
        &self,
        request: &SettlementSignerRequest,
    ) -> Result<(), SignerError> {
        if !matches!(
            request.transaction_class,
            TransactionClass::SettleErc20 | TransactionClass::SettleNative
        ) || !self.allowed_classes.contains(&request.transaction_class)
        {
            return Err(SignerError::ClassDenied);
        }
        if !request
            .transaction
            .to
            .eq_ignore_ascii_case(&self.factory_address)
            || !request
                .expected_factory
                .eq_ignore_ascii_case(&self.factory_address)
        {
            return Err(SignerError::WrongTarget);
        }
        if !request
            .settlement_destination
            .eq_ignore_ascii_case(&request.configured_merchant_destination)
        {
            return Err(SignerError::WrongSettlementDestination);
        }
        if matches!(request.transaction_class, TransactionClass::SettleErc20)
            && !request.transaction.value.is_zero()
        {
            return Err(SignerError::UnexpectedValue);
        }
        Ok(())
    }
}

#[async_trait]
pub trait RestrictedSigner: Send + Sync {
    async fn submit_recovery(&self, request: &ApprovedSignerRequest)
        -> Result<String, SignerError>;
    async fn submit_settlement(
        &self,
        request: &SettlementSignerRequest,
    ) -> Result<String, SignerError>;
}

/// Local/test-chain signer. It relies on an unlocked Anvil account but still applies the
/// same plan-hash, class and target restrictions before asking the node to submit a tx.
#[derive(Clone)]
pub struct TestnetKeySigner {
    pub policy: SignerPolicy,
    pub rpc_url: String,
    wallet: LocalWallet,
    client: reqwest::Client,
}

impl TestnetKeySigner {
    pub fn from_private_key(
        policy: SignerPolicy,
        rpc_url: impl Into<String>,
        private_key: &str,
    ) -> Result<Self, SignerError> {
        let wallet = private_key
            .parse::<LocalWallet>()
            .map_err(|_| SignerError::Rpc("invalid signer key".into()))?;
        Ok(Self {
            policy,
            rpc_url: rpc_url.into(),
            wallet,
            client: reqwest::Client::new(),
        })
    }

    async fn submit(&self, transaction: &PreparedTransaction) -> Result<String, SignerError> {
        let chain_id: String = self
            .client
            .post(&self.rpc_url)
            .json(&json!({"jsonrpc":"2.0","id":1,"method":"eth_chainId","params":[]}))
            .send()
            .await
            .map_err(|e| SignerError::Rpc(e.to_string()))?
            .json::<Value>()
            .await
            .map_err(|e| SignerError::Rpc(e.to_string()))?
            .get("result")
            .and_then(Value::as_str)
            .ok_or_else(|| SignerError::Rpc("missing chain id".into()))?
            .to_owned();
        let chain_id = u64::from_str_radix(chain_id.trim_start_matches("0x"), 16)
            .map_err(|_| SignerError::Rpc("invalid chain id".into()))?;
        let nonce: String = self.client.post(&self.rpc_url).json(&json!({"jsonrpc":"2.0","id":1,"method":"eth_getTransactionCount","params":[format!("{:#x}", self.wallet.address()),"pending"]})).send().await.map_err(|e| SignerError::Rpc(e.to_string()))?.json::<Value>().await.map_err(|e| SignerError::Rpc(e.to_string()))?.get("result").and_then(Value::as_str).ok_or_else(|| SignerError::Rpc("missing nonce".into()))?.to_owned();
        let gas_price: String = self
            .client
            .post(&self.rpc_url)
            .json(&json!({"jsonrpc":"2.0","id":1,"method":"eth_gasPrice","params":[]}))
            .send()
            .await
            .map_err(|e| SignerError::Rpc(e.to_string()))?
            .json::<Value>()
            .await
            .map_err(|e| SignerError::Rpc(e.to_string()))?
            .get("result")
            .and_then(Value::as_str)
            .ok_or_else(|| SignerError::Rpc("missing gas price".into()))?
            .to_owned();
        let from = format!("{:#x}", self.wallet.address());
        let value_hex = format!("0x{}", transaction.value.inner().to_str_radix(16));
        let gas_limit: String = self
            .client
            .post(&self.rpc_url)
            .json(&json!({"jsonrpc":"2.0","id":1,"method":"eth_estimateGas","params":[{"from":from,"to":transaction.to,"value":value_hex,"data":transaction.calldata_hex}]}))
            .send()
            .await
            .map_err(|e| SignerError::Rpc(e.to_string()))?
            .json::<Value>()
            .await
            .map_err(|e| SignerError::Rpc(e.to_string()))?
            .get("result")
            .and_then(Value::as_str)
            .ok_or_else(|| SignerError::Rpc("missing gas estimate".into()))?
            .to_owned();
        let gas_limit =
            ethers_core::types::U256::from_str_radix(gas_limit.trim_start_matches("0x"), 16)
                .map_err(|_| SignerError::Rpc("invalid gas estimate".into()))?
                * ethers_core::types::U256::from(120_u64)
                / ethers_core::types::U256::from(100_u64);
        let tx = ethers_core::types::TransactionRequest::new()
            .to(transaction
                .to
                .parse::<ethers_core::types::Address>()
                .map_err(|_| SignerError::WrongTarget)?)
            .data(
                hex::decode(transaction.calldata_hex.trim_start_matches("0x"))
                    .map_err(|_| SignerError::Rpc("invalid calldata".into()))?,
            )
            .value(
                transaction
                    .value
                    .inner()
                    .to_string()
                    .parse::<ethers_core::types::U256>()
                    .map_err(|_| SignerError::Rpc("invalid value".into()))?,
            )
            .nonce(
                ethers_core::types::U256::from_str_radix(nonce.trim_start_matches("0x"), 16)
                    .map_err(|_| SignerError::Rpc("invalid nonce".into()))?,
            )
            .gas_price(
                ethers_core::types::U256::from_str_radix(gas_price.trim_start_matches("0x"), 16)
                    .map_err(|_| SignerError::Rpc("invalid gas price".into()))?,
            )
            .gas(gas_limit)
            .chain_id(chain_id);
        let typed_transaction = tx.into();
        let signature = self
            .wallet
            .sign_transaction(&typed_transaction)
            .await
            .map_err(|e| SignerError::Rpc(e.to_string()))?;
        let raw = format!(
            "0x{}",
            hex::encode(typed_transaction.rlp_signed(&signature))
        );
        let response: Value = self
            .client
            .post(&self.rpc_url)
            .json(&json!({"jsonrpc":"2.0","id":1,"method":"eth_sendRawTransaction","params":[raw]}))
            .send()
            .await
            .map_err(|e| SignerError::Rpc(e.to_string()))?
            .json()
            .await
            .map_err(|e| SignerError::Rpc(e.to_string()))?;
        if let Some(error) = response.get("error") {
            return Err(SignerError::Rpc(error.to_string()));
        }
        response
            .get("result")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| SignerError::Rpc("missing tx hash".into()))
    }
}

pub struct DevUnlockedSigner {
    pub policy: SignerPolicy,
    pub rpc_url: String,
    pub from: String,
    client: reqwest::Client,
}

impl DevUnlockedSigner {
    #[must_use]
    pub fn new(policy: SignerPolicy, rpc_url: impl Into<String>, from: impl Into<String>) -> Self {
        Self {
            policy,
            rpc_url: rpc_url.into(),
            from: from.into(),
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl RestrictedSigner for TestnetKeySigner {
    async fn submit_recovery(
        &self,
        request: &ApprovedSignerRequest,
    ) -> Result<String, SignerError> {
        self.policy.validate(request)?;
        self.submit(&request.transaction).await
    }

    async fn submit_settlement(
        &self,
        request: &SettlementSignerRequest,
    ) -> Result<String, SignerError> {
        self.policy.validate_settlement(request)?;
        self.submit(&request.transaction).await
    }
}

#[async_trait]
impl RestrictedSigner for DevUnlockedSigner {
    async fn submit_recovery(
        &self,
        request: &ApprovedSignerRequest,
    ) -> Result<String, SignerError> {
        self.policy.validate(request)?;
        let result: Value = self.client.post(&self.rpc_url).json(&json!({"jsonrpc":"2.0","id":1,"method":"eth_sendTransaction","params":[{"from":self.from,"to":request.transaction.to,"value":format!("0x{}", request.transaction.value.inner().to_str_radix(16)),"data":request.transaction.calldata_hex}]})).send().await.map_err(|e| SignerError::Rpc(e.to_string()))?.json().await.map_err(|e| SignerError::Rpc(e.to_string()))?;
        if let Some(error) = result.get("error") {
            return Err(SignerError::Rpc(error.to_string()));
        }
        result
            .get("result")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| SignerError::Rpc("missing tx hash".into()))
    }

    async fn submit_settlement(
        &self,
        request: &SettlementSignerRequest,
    ) -> Result<String, SignerError> {
        self.policy.validate_settlement(request)?;
        let result: Value = self.client.post(&self.rpc_url).json(&json!({"jsonrpc":"2.0","id":1,"method":"eth_sendTransaction","params":[{"from":self.from,"to":request.transaction.to,"value":format!("0x{}", request.transaction.value.inner().to_str_radix(16)),"data":request.transaction.calldata_hex}]})).send().await.map_err(|e| SignerError::Rpc(e.to_string()))?.json().await.map_err(|e| SignerError::Rpc(e.to_string()))?;
        if let Some(error) = result.get("error") {
            return Err(SignerError::Rpc(error.to_string()));
        }
        result
            .get("result")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| SignerError::Rpc("missing tx hash".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use flowpay_domain::{AtomicAmount, ChainKey};
    use std::str::FromStr;
    #[test]
    fn rejects_replayed_or_wrong_target_requests() {
        let policy = SignerPolicy {
            allowed_classes: [TransactionClass::RecoverErc20].into_iter().collect(),
            factory_address: "0xfac".into(),
        };
        let mut request = ApprovedSignerRequest {
            plan_id: RecoveryPlanId::new(),
            approval_id: ApprovalId::new(),
            approved_plan_hash: "abc".into(),
            computed_plan_hash: "abc".into(),
            approval_reserved_for_execution: true,
            transaction_class: TransactionClass::RecoverErc20,
            transaction: PreparedTransaction {
                transaction_class: "RECOVER_ERC20".into(),
                chain: ChainKey::Bsc,
                to: "0xfac".into(),
                value: AtomicAmount::from_str("0").unwrap(),
                calldata_hex: "0x1234".into(),
            },
            expected_factory: "0xfac".into(),
        };
        assert!(policy.validate(&request).is_ok());
        request.approval_reserved_for_execution = false;
        assert!(matches!(
            policy.validate(&request),
            Err(SignerError::ApprovalNotReserved)
        ));
        request.approval_reserved_for_execution = true;
        request.transaction.to = "0xevil".into();
        assert!(matches!(
            policy.validate(&request),
            Err(SignerError::WrongTarget)
        ));
    }
}

use alloy_primitives::keccak256;
use async_trait::async_trait;
use flowpay_chains::{
    ChainAdapter, ChainError, ChainHealth, ChainIdentity, ChainTransaction, ConfirmationStatus,
    CounterfactualEvmAdapter, FactoryVerification, PreparedTransaction, SimulationResult,
    TokenMetadata, TransactionReceipt, TransferEvent,
};
use flowpay_domain::{AddressRef, AtomicAmount};
use serde::Deserialize;
use serde_json::{json, Value};

const TRANSFER_TOPIC: &str = "ddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef";

#[derive(Clone, Debug)]
pub struct Create3AddressDeriver {
    factory: [u8; 20],
    proxy_creation_code_hash: [u8; 32],
}

impl Create3AddressDeriver {
    pub fn new(factory_hex: &str, proxy_creation_code_hash_hex: &str) -> Result<Self, ChainError> {
        Ok(Self {
            factory: parse_fixed::<20>(factory_hex)?,
            proxy_creation_code_hash: parse_fixed::<32>(proxy_creation_code_hash_hex)?,
        })
    }

    #[must_use]
    pub fn factory_hex(&self) -> String {
        format!("0x{}", hex::encode(self.factory))
    }

    #[must_use]
    pub fn proxy_address(&self, salt: [u8; 32]) -> [u8; 20] {
        let mut input = Vec::with_capacity(85);
        input.push(0xff);
        input.extend_from_slice(&self.factory);
        input.extend_from_slice(&salt);
        input.extend_from_slice(&self.proxy_creation_code_hash);
        let hash = keccak256(input);
        let mut out = [0_u8; 20];
        out.copy_from_slice(&hash[12..]);
        out
    }

    #[must_use]
    pub fn checkout_address(&self, salt: [u8; 32]) -> [u8; 20] {
        let proxy = self.proxy_address(salt);
        let mut rlp = [0_u8; 23];
        rlp[0] = 0xd6;
        rlp[1] = 0x94;
        rlp[2..22].copy_from_slice(&proxy);
        rlp[22] = 0x01;
        let hash = keccak256(rlp);
        let mut out = [0_u8; 20];
        out.copy_from_slice(&hash[12..]);
        out
    }

    #[must_use]
    pub fn checkout_hex(&self, salt: [u8; 32]) -> String {
        format!("0x{}", hex::encode(self.checkout_address(salt)))
    }
}

fn parse_fixed<const N: usize>(value: &str) -> Result<[u8; N], ChainError> {
    let raw = hex::decode(value.strip_prefix("0x").unwrap_or(value))
        .map_err(|e| ChainError::InvalidData(e.to_string()))?;
    raw.try_into()
        .map_err(|_| ChainError::InvalidData(format!("expected {N} bytes")))
}

fn topic_address(address: &str) -> Result<String, ChainError> {
    let bytes = parse_fixed::<20>(address)?;
    Ok(format!("0x{}{}", "00".repeat(12), hex::encode(bytes)))
}

fn quantity_u64(value: &str) -> Result<u64, ChainError> {
    u64::from_str_radix(value.trim_start_matches("0x"), 16)
        .map_err(|e| ChainError::InvalidData(e.to_string()))
}

#[derive(Clone)]
pub struct RpcEvmAdapter {
    identity: ChainIdentity,
    rpc_url: String,
    client: reqwest::Client,
    provider_id: String,
    deriver: Create3AddressDeriver,
    expected_factory_runtime_hash: Option<String>,
    expected_operator: Option<String>,
}

impl RpcEvmAdapter {
    pub fn new(
        identity: ChainIdentity,
        rpc_url: impl Into<String>,
        deriver: Create3AddressDeriver,
        expected_factory_runtime_hash: Option<String>,
        expected_operator: Option<String>,
    ) -> Self {
        Self {
            identity,
            rpc_url: rpc_url.into(),
            client: reqwest::Client::new(),
            provider_id: "primary".into(),
            deriver,
            expected_factory_runtime_hash,
            expected_operator,
        }
    }

    async fn rpc(&self, method: &str, params: Value) -> Result<Value, ChainError> {
        let response = self
            .client
            .post(&self.rpc_url)
            .json(&json!({"jsonrpc":"2.0","id":1,"method":method,"params":params}))
            .send()
            .await
            .map_err(|e| ChainError::ProviderUnavailable(e.to_string()))?;
        let status = response.status();
        let body: Value = response
            .json()
            .await
            .map_err(|e| ChainError::InvalidData(e.to_string()))?;
        if !status.is_success() {
            return Err(ChainError::ProviderUnavailable(format!("HTTP {status}")));
        }
        if let Some(error) = body.get("error") {
            return Err(ChainError::InvalidData(error.to_string()));
        }
        body.get("result")
            .cloned()
            .ok_or_else(|| ChainError::InvalidData("missing JSON-RPC result".into()))
    }

    async fn block_number(&self) -> Result<u64, ChainError> {
        quantity_u64(
            self.rpc("eth_blockNumber", json!([]))
                .await?
                .as_str()
                .ok_or_else(|| ChainError::InvalidData("invalid block number".into()))?,
        )
    }

    async fn eth_call(&self, to: &str, data: &str) -> Result<String, ChainError> {
        self.rpc("eth_call", json!([{"to":to,"data":data},"latest"]))
            .await?
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| ChainError::InvalidData("invalid eth_call".into()))
    }

    async fn code(&self, address: &str) -> Result<String, ChainError> {
        self.rpc("eth_getCode", json!([address, "latest"]))
            .await?
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| ChainError::InvalidData("invalid code response".into()))
    }
}

#[derive(Deserialize)]
struct RpcTx {
    hash: String,
    from: String,
    to: Option<String>,
    value: String,
    #[serde(rename = "blockNumber")]
    block_number: Option<String>,
    #[serde(rename = "blockHash")]
    block_hash: Option<String>,
}
#[derive(Deserialize)]
struct RpcReceipt {
    #[serde(rename = "transactionHash")]
    transaction_hash: String,
    #[serde(rename = "blockNumber")]
    block_number: String,
    #[serde(rename = "blockHash")]
    block_hash: String,
    status: Option<String>,
    #[serde(default)]
    logs: Vec<RpcLog>,
}
#[derive(Deserialize)]
struct RpcLog {
    address: String,
    topics: Vec<String>,
    data: String,
    #[serde(rename = "transactionHash")]
    transaction_hash: String,
    #[serde(rename = "blockNumber")]
    block_number: String,
    #[serde(rename = "blockHash")]
    block_hash: String,
    #[serde(rename = "logIndex")]
    log_index: String,
    removed: Option<bool>,
}

#[async_trait]
impl ChainAdapter for RpcEvmAdapter {
    fn identity(&self) -> &ChainIdentity {
        &self.identity
    }

    async fn health(&self) -> Result<ChainHealth, ChainError> {
        let chain_id = self.rpc("eth_chainId", json!([])).await?;
        let actual = quantity_u64(
            chain_id
                .as_str()
                .ok_or_else(|| ChainError::InvalidData("invalid chain id".into()))?,
        )?;
        if self.identity.numeric_chain_id != Some(actual) {
            return Err(ChainError::ChainIdentityMismatch);
        }
        Ok(ChainHealth {
            healthy: true,
            latest_height: self.block_number().await?,
            provider_id: self.provider_id.clone(),
        })
    }

    async fn transaction(&self, hash: &str) -> Result<ChainTransaction, ChainError> {
        let result = self.rpc("eth_getTransactionByHash", json!([hash])).await?;
        if result.is_null() {
            return Err(ChainError::TransactionNotFound);
        }
        let tx: RpcTx =
            serde_json::from_value(result).map_err(|e| ChainError::InvalidData(e.to_string()))?;
        Ok(ChainTransaction {
            hash: tx.hash,
            from: tx.from,
            to: tx.to,
            block_number: tx.block_number.as_deref().map(quantity_u64).transpose()?,
            block_hash: tx.block_hash,
            success: None,
            native_value: AtomicAmount::from_hex_quantity(&tx.value)
                .map_err(|e| ChainError::InvalidData(e.to_string()))?,
        })
    }

    async fn receipt(&self, hash: &str) -> Result<TransactionReceipt, ChainError> {
        let result = self.rpc("eth_getTransactionReceipt", json!([hash])).await?;
        if result.is_null() {
            return Err(ChainError::TransactionNotFound);
        }
        let r: RpcReceipt =
            serde_json::from_value(result).map_err(|e| ChainError::InvalidData(e.to_string()))?;
        Ok(TransactionReceipt {
            transaction_hash: r.transaction_hash,
            block_number: quantity_u64(&r.block_number)?,
            block_hash: r.block_hash,
            success: r.status.as_deref() != Some("0x0"),
        })
    }

    async fn transaction_transfers(&self, hash: &str) -> Result<Vec<TransferEvent>, ChainError> {
        let result = self.rpc("eth_getTransactionReceipt", json!([hash])).await?;
        if result.is_null() {
            return Err(ChainError::TransactionNotFound);
        }
        let r: RpcReceipt =
            serde_json::from_value(result).map_err(|e| ChainError::InvalidData(e.to_string()))?;
        let mut out = Vec::new();
        for log in r.logs.into_iter().filter(|l| !l.removed.unwrap_or(false)) {
            if log.topics.len() < 3
                || !log.topics[0]
                    .trim_start_matches("0x")
                    .eq_ignore_ascii_case(TRANSFER_TOPIC)
            {
                continue;
            }
            let from_topic = log.topics[1].trim_start_matches("0x");
            let to_topic = log.topics[2].trim_start_matches("0x");
            if from_topic.len() != 64 || to_topic.len() != 64 {
                continue;
            }
            out.push(TransferEvent {
                chain: self.identity.key.clone(),
                tx_hash: r.transaction_hash.clone(),
                block_number: quantity_u64(&r.block_number)?,
                block_hash: r.block_hash.clone(),
                log_index: Some(quantity_u64(&log.log_index)?),
                from: format!("0x{}", &from_topic[24..]),
                to: format!("0x{}", &to_topic[24..]),
                token_contract: Some(log.address),
                amount: AtomicAmount::from_hex_quantity(&log.data)
                    .map_err(|e| ChainError::InvalidData(e.to_string()))?,
            });
        }
        if out.is_empty() {
            let tx = self.transaction(hash).await?;
            if !tx.native_value.is_zero() {
                if let Some(to) = tx.to {
                    out.push(TransferEvent {
                        chain: self.identity.key.clone(),
                        tx_hash: tx.hash,
                        block_number: tx.block_number.unwrap_or_default(),
                        block_hash: tx.block_hash.unwrap_or_default(),
                        log_index: None,
                        from: tx.from,
                        to,
                        token_contract: None,
                        amount: tx.native_value,
                    });
                }
            }
        }
        Ok(out)
    }

    async fn confirmation_status(
        &self,
        block_number: u64,
        block_hash: &str,
        required_confirmations: u64,
    ) -> Result<ConfirmationStatus, ChainError> {
        let block = self
            .rpc(
                "eth_getBlockByNumber",
                json!([format!("0x{block_number:x}"), false]),
            )
            .await?;
        let canonical = block
            .get("hash")
            .and_then(Value::as_str)
            .ok_or_else(|| ChainError::InvalidData("block missing hash".into()))?;
        if !canonical.eq_ignore_ascii_case(block_hash) {
            return Err(ChainError::NonCanonical);
        }
        let latest = self.block_number().await?;
        let confirmations = latest.saturating_sub(block_number).saturating_add(1);
        Ok(ConfirmationStatus {
            observed_block: block_number,
            canonical_block_hash: canonical.to_owned(),
            latest_block: latest,
            confirmations,
            final_enough: confirmations >= required_confirmations,
        })
    }

    async fn transfers_to(
        &self,
        address: &AddressRef,
        from_block: u64,
        to_block: u64,
    ) -> Result<Vec<TransferEvent>, ChainError> {
        if address.chain != self.identity.key {
            return Err(ChainError::ChainIdentityMismatch);
        }
        let logs = self.rpc("eth_getLogs", json!([{"fromBlock":format!("0x{from_block:x}"),"toBlock":format!("0x{to_block:x}"),"topics":[format!("0x{TRANSFER_TOPIC}"),null,topic_address(&address.value)?]}])).await?;
        let logs: Vec<RpcLog> =
            serde_json::from_value(logs).map_err(|e| ChainError::InvalidData(e.to_string()))?;
        let mut out = Vec::new();
        for log in logs.into_iter().filter(|l| !l.removed.unwrap_or(false)) {
            if log.topics.len() < 3 {
                continue;
            }
            let from = format!("0x{}", &log.topics[1].trim_start_matches("0x")[24..]);
            let amount = AtomicAmount::from_hex_quantity(&log.data)
                .map_err(|e| ChainError::InvalidData(e.to_string()))?;
            out.push(TransferEvent {
                chain: self.identity.key.clone(),
                tx_hash: log.transaction_hash,
                block_number: quantity_u64(&log.block_number)?,
                block_hash: log.block_hash,
                log_index: Some(quantity_u64(&log.log_index)?),
                from,
                to: address.value.clone(),
                token_contract: Some(log.address),
                amount,
            });
        }
        // Native transfers require scanning transactions because they do not emit logs.
        for height in from_block..=to_block {
            let block = self
                .rpc(
                    "eth_getBlockByNumber",
                    json!([format!("0x{height:x}"), true]),
                )
                .await?;
            if let Some(txs) = block.get("transactions").and_then(Value::as_array) {
                for tx in txs {
                    let to = tx.get("to").and_then(Value::as_str);
                    let value = tx.get("value").and_then(Value::as_str).unwrap_or("0x0");
                    if to.is_some_and(|v| v.eq_ignore_ascii_case(&address.value)) && value != "0x0"
                    {
                        out.push(TransferEvent {
                            chain: self.identity.key.clone(),
                            tx_hash: tx
                                .get("hash")
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_owned(),
                            block_number: height,
                            block_hash: block
                                .get("hash")
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_owned(),
                            log_index: None,
                            from: tx
                                .get("from")
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_owned(),
                            to: address.value.clone(),
                            token_contract: None,
                            amount: AtomicAmount::from_hex_quantity(value)
                                .map_err(|e| ChainError::InvalidData(e.to_string()))?,
                        });
                    }
                }
            }
        }
        Ok(out)
    }

    async fn token_metadata(&self, contract: &str) -> Result<TokenMetadata, ChainError> {
        let symbol_raw = self.eth_call(contract, "0x95d89b41").await?;
        let decimals_raw = self.eth_call(contract, "0x313ce567").await?;
        let symbol = decode_abi_string(&symbol_raw).unwrap_or_else(|| "UNKNOWN".into());
        let decimals_u64 = quantity_u64(&decimals_raw)?;
        let decimals = u8::try_from(decimals_u64)
            .map_err(|_| ChainError::InvalidData("token decimals > 255".into()))?;
        let code = self.code(contract).await?;
        Ok(TokenMetadata {
            contract: contract.to_owned(),
            symbol,
            decimals,
            code_hash: Some(format!(
                "0x{}",
                hex::encode(keccak256(
                    hex::decode(code.trim_start_matches("0x"))
                        .map_err(|e| ChainError::InvalidData(e.to_string()))?
                ))
            )),
        })
    }

    async fn token_balance(
        &self,
        token_contract: &str,
        address: &str,
    ) -> Result<AtomicAmount, ChainError> {
        let padded = topic_address(address)?;
        let data = format!("0x70a08231{}", padded.trim_start_matches("0x"));
        let result = self.eth_call(token_contract, &data).await?;
        AtomicAmount::from_hex_quantity(&result).map_err(|e| ChainError::InvalidData(e.to_string()))
    }

    async fn native_balance(&self, address: &str) -> Result<AtomicAmount, ChainError> {
        let result = self
            .rpc("eth_getBalance", json!([address, "latest"]))
            .await?;
        AtomicAmount::from_hex_quantity(
            result
                .as_str()
                .ok_or_else(|| ChainError::InvalidData("invalid balance".into()))?,
        )
        .map_err(|e| ChainError::InvalidData(e.to_string()))
    }

    async fn has_code(&self, address: &str) -> Result<bool, ChainError> {
        let code = self.code(address).await?;
        Ok(code != "0x" && code != "0x0")
    }

    async fn simulate(&self, tx: &PreparedTransaction) -> Result<SimulationResult, ChainError> {
        let mut obj = json!({"to":tx.to,"value":format!("0x{}", tx.value.inner().to_str_radix(16)),"data":tx.calldata_hex});
        if let Some(operator) = &self.expected_operator {
            obj["from"] = Value::String(operator.clone());
        }
        let call = self.rpc("eth_call", json!([obj.clone(), "latest"])).await;
        if let Err(error) = call {
            return Ok(SimulationResult {
                success: false,
                gas_estimate: None,
                revert_reason: Some(error.to_string()),
                balance_deltas_verified: false,
            });
        }
        let gas_estimate = match self.rpc("eth_estimateGas", json!([obj])).await {
            Ok(value) => Some(
                AtomicAmount::from_hex_quantity(
                    value
                        .as_str()
                        .ok_or_else(|| ChainError::InvalidData("invalid gas estimate".into()))?,
                )
                .map_err(|e| ChainError::InvalidData(e.to_string()))?,
            ),
            Err(_) => None,
        };
        Ok(SimulationResult {
            success: true,
            gas_estimate,
            revert_reason: None,
            balance_deltas_verified: false,
        })
    }
}

#[async_trait]
impl CounterfactualEvmAdapter for RpcEvmAdapter {
    async fn compute_checkout_address(
        &self,
        factory: &str,
        salt_hex: &str,
    ) -> Result<String, ChainError> {
        if !factory.eq_ignore_ascii_case(&self.deriver.factory_hex()) {
            return Err(ChainError::InvalidData("unexpected factory".into()));
        }
        let salt = parse_fixed::<32>(salt_hex)?;
        Ok(self.deriver.checkout_hex(salt))
    }

    async fn verify_factory(&self, factory: &str) -> Result<FactoryVerification, ChainError> {
        let code = self.code(factory).await?;
        let has_code = code != "0x" && code != "0x0";
        let code_bytes = hex::decode(code.trim_start_matches("0x"))
            .map_err(|e| ChainError::InvalidData(e.to_string()))?;
        let runtime_hash = format!("0x{}", hex::encode(keccak256(code_bytes)));
        let hash_matches = self
            .expected_factory_runtime_hash
            .as_ref()
            .is_none_or(|expected| expected.eq_ignore_ascii_case(&runtime_hash));
        let operator_matches = if let Some(expected) = &self.expected_operator {
            let selector = &keccak256("operator()".as_bytes())[..4];
            let result = self
                .eth_call(factory, &format!("0x{}", hex::encode(selector)))
                .await?;
            result.len() >= 42
                && result[result.len() - 40..]
                    .eq_ignore_ascii_case(expected.trim_start_matches("0x"))
        } else {
            true
        };
        let zero = [0_u8; 32];
        let local = self.deriver.checkout_hex(zero);
        let selector = &keccak256("computeCheckoutAddress(bytes32)".as_bytes())[..4];
        let remote = self
            .eth_call(
                factory,
                &format!("0x{}{}", hex::encode(selector), "00".repeat(32)),
            )
            .await?;
        let prediction_matches = remote.len() >= 42
            && remote[remote.len() - 40..].eq_ignore_ascii_case(local.trim_start_matches("0x"));
        Ok(FactoryVerification {
            factory: factory.to_owned(),
            has_code,
            runtime_code_hash: Some(runtime_hash),
            expected_code_hash_matches: hash_matches,
            deployer_authority_matches: operator_matches,
            prediction_vector_matches: prediction_matches,
            recovery_capable: has_code && hash_matches && operator_matches && prediction_matches,
        })
    }
}

fn decode_abi_string(value: &str) -> Option<String> {
    let bytes = hex::decode(value.trim_start_matches("0x")).ok()?;
    if bytes.len() == 32 {
        let trimmed = bytes
            .iter()
            .copied()
            .take_while(|b| *b != 0)
            .collect::<Vec<_>>();
        return String::from_utf8(trimmed).ok();
    }
    if bytes.len() < 64 {
        return None;
    }
    let len = usize::from_str_radix(&hex::encode(&bytes[32..64]), 16).ok()?;
    if bytes.len() < 64 + len {
        return None;
    }
    String::from_utf8(bytes[64..64 + len].to_vec()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create3_derivation_is_deterministic_and_chain_agnostic() {
        let d = Create3AddressDeriver::new(
            "0x1111111111111111111111111111111111111111",
            "0x2222222222222222222222222222222222222222222222222222222222222222",
        )
        .unwrap();
        let salt = [7_u8; 32];
        assert_eq!(d.checkout_hex(salt), d.checkout_hex(salt));
        assert_ne!(d.checkout_hex(salt), d.checkout_hex([8_u8; 32]));
    }
}

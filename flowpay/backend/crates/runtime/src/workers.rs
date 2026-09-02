use crate::{
    agent_runtime::DatabaseAgentTools,
    state::{AppState, ChainRuntime},
};
use flowpay_chains::ChainAdapter;
use flowpay_domain::{AddressRef, AtomicAmount, ChainKey, Payment, PaymentState};
use flowpay_messaging::{
    enqueue_command_at_tx, InboxReservation, MessagingError, OutboxStore, RabbitCommandConsumer,
};
use flowpay_payments::{reconcile, ObservedDeposit, ReconciliationInput};
use flowpay_persistence::StoredDeposit;
use flowpay_recovery::{build_factory_erc20_sweep, build_factory_native_sweep};
use flowpay_signer::{
    DevUnlockedSigner, RestrictedSigner, SettlementSignerRequest, SignerPolicy, TestnetKeySigner,
    TransactionClass,
};
use serde_json::{json, Value};
use sqlx::Row;
use std::{collections::BTreeSet, time::Duration as StdDuration};
use time::OffsetDateTime;
use tracing::{error, info, warn};

pub fn spawn_periodic(state: AppState) {
    let settlement = state.clone();
    tokio::spawn(async move {
        loop {
            if let Err(e) = settlement_tick(&settlement).await {
                error!(error=%e,"settlement tick failed");
            }
            tokio::time::sleep(StdDuration::from_secs(2)).await;
        }
    });
    let webhooks = state.clone();
    tokio::spawn(async move {
        loop {
            if let Err(e) = webhook_tick(&webhooks).await {
                error!(error=%e,"webhook worker tick failed");
            }
            tokio::time::sleep(StdDuration::from_secs(2)).await;
        }
    });
    let agent = state.clone();
    tokio::spawn(async move {
        loop {
            if let Err(e) = agent_tick(&agent).await {
                error!(error=%e,"agent worker tick failed");
            }
            tokio::time::sleep(StdDuration::from_secs(10)).await;
        }
    });
}

pub async fn run(state: AppState) -> anyhow::Result<()> {
    spawn_periodic(state.clone());
    let broker_state = state.clone();
    tokio::spawn(async move {
        loop {
            let consumer = match RabbitCommandConsumer::connect(&broker_state.config.rabbitmq_url)
                .await
            {
                Ok(consumer) => consumer,
                Err(error) => {
                    warn!(error=%error,"RabbitMQ unavailable; DB reconciliation loops remain active");
                    tokio::time::sleep(StdDuration::from_secs(5)).await;
                    continue;
                }
            };
            let state_for_handler = broker_state.clone();
            let result = consumer
                .run(
                    "flowpay.commands.worker",
                    &[
                        "payment.monitor.start",
                        "payment.reconcile",
                        "payment.settlement.execute",
                        "claim.investigate",
                        "recovery.simulate",
                        "recovery.execute",
                        "recovery.verify",
                        "webhook.deliver",
                        "webhook.retry",
                    ],
                    move |envelope| {
                        let state = state_for_handler.clone();
                        async move { handle_command(&state, envelope).await }
                    },
                )
                .await;
            if let Err(error) = result {
                warn!(error=%error,"RabbitMQ consumer stopped; reconnecting");
            }
            tokio::time::sleep(StdDuration::from_secs(2)).await;
        }
    });
    tokio::signal::ctrl_c().await?;
    info!("FlowPay worker shutting down");
    Ok(())
}

async fn handle_command(
    state: &AppState,
    envelope: flowpay_messaging::MessageEnvelope,
) -> Result<(), MessagingError> {
    const CONSUMER: &str = "flowpay-worker";
    let inbox = OutboxStore::new(state.store.pool().clone());
    match inbox
        .reserve_message(CONSUMER, envelope.event_id, 120, 5)
        .await?
    {
        InboxReservation::Acquired => {}
        InboxReservation::Completed => return Ok(()),
        InboxReservation::Busy => {
            return Err(MessagingError::Rabbit(
                "command reservation is active or retry is not due".into(),
            ))
        }
        InboxReservation::Exhausted => {
            return Err(MessagingError::Rabbit(
                "command exhausted its durable retry budget".into(),
            ))
        }
    }
    let result = match envelope.event_type.as_str() {
        "payment.monitor.start" | "payment.reconcile" => payment_monitor_tick(state).await,
        "payment.settlement.execute" => settlement_tick(state).await,
        "claim.investigate" | "recovery.simulate" | "recovery.execute" | "recovery.verify" => {
            agent_tick(state).await
        }
        "webhook.deliver" | "webhook.retry" => webhook_tick(state).await,
        other => {
            warn!(event_type=%other,"unknown operational command; moving toward DLQ");
            Err(anyhow::anyhow!("unknown operational command: {other}"))
        }
    };
    match result {
        Ok(()) => inbox.complete_message(CONSUMER, envelope.event_id).await,
        Err(error) => {
            let message = error.to_string();
            inbox
                .fail_message(CONSUMER, envelope.event_id, &message)
                .await?;
            Err(MessagingError::Rabbit(message))
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DepositRevalidation {
    Stable,
    Reorged,
    Unavailable,
}

async fn revalidate_persisted_deposits(
    state: &AppState,
    payment: &Payment,
    chain: &ChainKey,
    runtime: &ChainRuntime,
) -> anyhow::Result<DepositRevalidation> {
    let rows = sqlx::query(
        "SELECT tx_hash, observed_block_hash FROM deposits WHERE payment_id=$1 AND chain=$2 AND confirmation_status<>'ORPHANED' ORDER BY created_at,id",
    )
    .bind(payment.id.0)
    .bind(chain.to_string())
    .fetch_all(state.store.pool())
    .await?;

    let mut reorged = false;
    for row in rows {
        let tx_hash: String = row.try_get("tx_hash")?;
        let observed_block_hash: String = row.try_get("observed_block_hash")?;
        let receipt = match runtime.adapter.receipt(&tx_hash).await {
            Ok(receipt) => receipt,
            Err(flowpay_chains::ChainError::TransactionNotFound) => {
                mark_deposit_orphaned(state, payment, chain, &tx_hash).await?;
                reorged = true;
                continue;
            }
            Err(e) => {
                warn!(payment_id=%payment.id.0, tx_hash=%tx_hash, error=%e, "deposit canonicality could not be revalidated");
                return Ok(DepositRevalidation::Unavailable);
            }
        };

        if !receipt.success
            || !receipt
                .block_hash
                .eq_ignore_ascii_case(&observed_block_hash)
        {
            mark_deposit_orphaned(state, payment, chain, &tx_hash).await?;
            reorged = true;
            continue;
        }

        let confirmation = match runtime
            .adapter
            .confirmation_status(
                receipt.block_number,
                &receipt.block_hash,
                payment.required_confirmations,
            )
            .await
        {
            Ok(value) => value,
            Err(flowpay_chains::ChainError::NonCanonical) => {
                mark_deposit_orphaned(state, payment, chain, &tx_hash).await?;
                reorged = true;
                continue;
            }
            Err(e) => {
                warn!(payment_id=%payment.id.0, tx_hash=%tx_hash, error=%e, "deposit confirmation status unavailable during canonicality check");
                return Ok(DepositRevalidation::Unavailable);
            }
        };

        sqlx::query(
            "UPDATE deposits SET confirmation_status=$4,confirmations=$5,observed_block_number=$6::numeric,observed_block_hash=$7,updated_at=now() WHERE payment_id=$1 AND chain=$2 AND tx_hash=$3 AND confirmation_status<>'ORPHANED'",
        )
        .bind(payment.id.0)
        .bind(chain.to_string())
        .bind(&tx_hash)
        .bind(if confirmation.final_enough { "FINAL" } else { "CONFIRMING" })
        .bind(i32::try_from(confirmation.confirmations).unwrap_or(i32::MAX))
        .bind(receipt.block_number.to_string())
        .bind(&receipt.block_hash)
        .execute(state.store.pool())
        .await?;
        sqlx::query(
            "UPDATE chain_transactions SET block_number=$3::numeric,block_hash=$4,tx_status='SUCCESS',canonical=true,verified_at=now() WHERE chain=$1 AND tx_hash=$2",
        )
        .bind(chain.to_string())
        .bind(&tx_hash)
        .bind(receipt.block_number.to_string())
        .bind(&receipt.block_hash)
        .execute(state.store.pool())
        .await?;
    }

    Ok(if reorged {
        DepositRevalidation::Reorged
    } else {
        DepositRevalidation::Stable
    })
}

async fn mark_deposit_orphaned(
    state: &AppState,
    payment: &Payment,
    chain: &ChainKey,
    tx_hash: &str,
) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE deposits SET confirmation_status='ORPHANED',updated_at=now() WHERE payment_id=$1 AND chain=$2 AND tx_hash=$3 AND confirmation_status<>'ORPHANED'",
    )
    .bind(payment.id.0)
    .bind(chain.to_string())
    .bind(tx_hash)
    .execute(state.store.pool())
    .await?;
    sqlx::query(
        "UPDATE chain_transactions SET canonical=false,verified_at=now() WHERE chain=$1 AND tx_hash=$2",
    )
    .bind(chain.to_string())
    .bind(tx_hash)
    .execute(state.store.pool())
    .await?;
    warn!(payment_id=%payment.id.0, chain=%chain, tx_hash=%tx_hash, "persisted deposit marked orphaned after canonicality revalidation");
    Ok(())
}

async fn rewind_payment_after_reorg(state: &AppState, payment: &Payment) -> anyhow::Result<()> {
    match payment.state {
        PaymentState::Confirmed | PaymentState::Overpaid => {
            state
                .store
                .set_payment_state(
                    payment.id,
                    PaymentState::Confirming,
                    "canonical_deposit_reorged",
                    Some(&payment.expected_chain),
                    None,
                )
                .await?;
        }
        PaymentState::Settling => {
            // A reorg racing an already-started settlement is no longer safe to auto-rewind.
            // Stop the automatic path and require operator inspection instead of risking a
            // second settlement attempt against an uncertain canonical history.
            state
                .store
                .set_payment_state(
                    payment.id,
                    PaymentState::Failed,
                    "reorg_during_settlement",
                    Some(&payment.expected_chain),
                    None,
                )
                .await?;
        }
        _ => {}
    }
    Ok(())
}

async fn payment_monitor_tick(state: &AppState) -> anyhow::Result<()> {
    for (chain, runtime) in &state.chains {
        let health = match runtime.adapter.health().await {
            Ok(v) => v,
            Err(e) => {
                warn!(%chain,error=%e,"chain unhealthy");
                continue;
            }
        };
        let ids = state.store.list_monitorable_payment_ids(chain).await?;
        for id in ids {
            let mut payment = match state.store.get_payment_by_id(id).await {
                Ok(v) => v,
                Err(e) => {
                    warn!(payment_id=%id.0,error=%e,"payment load failed");
                    continue;
                }
            };
            if OffsetDateTime::now_utc() > payment.expires_at
                && matches!(
                    payment.state,
                    PaymentState::Waiting | PaymentState::PartiallyPaid
                )
            {
                let _ = state
                    .store
                    .set_payment_state(
                        payment.id,
                        PaymentState::Expired,
                        "payment_expired",
                        None,
                        None,
                    )
                    .await;
                continue;
            }
            match revalidate_persisted_deposits(state, &payment, chain, runtime).await? {
                DepositRevalidation::Reorged => {
                    // A reorg on a non-expected chain only invalidates that chain's
                    // observation; it must not rewind a valid expected-chain payment.
                    if *chain == payment.expected_chain {
                        rewind_payment_after_reorg(state, &payment).await?;
                        payment = state.store.get_payment_by_id(payment.id).await?;
                    }
                }
                DepositRevalidation::Unavailable
                    if matches!(
                        payment.state,
                        PaymentState::Confirmed | PaymentState::Overpaid | PaymentState::Settling
                    ) =>
                {
                    // Never advance a value-moving state while canonicality cannot be independently checked.
                    continue;
                }
                DepositRevalidation::Stable | DepositRevalidation::Unavailable => {}
            }
            // Every configured chain must be scanned, even after an expected-chain
            // deposit was confirmed. A later transfer on another chain/token is an
            // exception and must be persisted for claim investigation.
            if payment.state == PaymentState::Settling {
                continue;
            }
            let cursor = state
                .store
                .monitor_cursor(payment.id, chain)
                .await?
                .unwrap_or_else(|| health.latest_height.saturating_sub(8));
            let from = cursor.saturating_sub(3);
            let to = health.latest_height.min(from.saturating_add(250));
            if to < from {
                continue;
            }
            let transfers = match runtime
                .adapter
                .transfers_to(
                    &AddressRef {
                        chain: chain.clone(),
                        value: state
                            .store
                            .checkout_address_for_chain(payment.id, chain)
                            .await?,
                    },
                    from,
                    to,
                )
                .await
            {
                Ok(v) => v,
                Err(e) => {
                    warn!(payment_id=%payment.id.0,%chain,error=%e,"transfer scan failed");
                    continue;
                }
            };
            let mut saw_new = false;
            for transfer in transfers {
                let conf = match runtime
                    .adapter
                    .confirmation_status(
                        transfer.block_number,
                        &transfer.block_hash,
                        payment.required_confirmations,
                    )
                    .await
                {
                    Ok(v) => v,
                    Err(flowpay_chains::ChainError::NonCanonical) => {
                        mark_deposit_orphaned(state, &payment, chain, &transfer.tx_hash).await?;
                        continue;
                    }
                    Err(e) => {
                        warn!(tx_hash=%transfer.tx_hash,error=%e,"confirmation verification failed");
                        continue;
                    }
                };
                let receipt = match runtime.adapter.receipt(&transfer.tx_hash).await {
                    Ok(v) => v,
                    Err(e) => {
                        warn!(tx_hash=%transfer.tx_hash,error=%e,"receipt verification failed");
                        continue;
                    }
                };
                if !receipt.success {
                    continue;
                }
                let (symbol, decimals) = if let Some(token) = transfer.token_contract.as_deref() {
                    match runtime.adapter.token_metadata(token).await {
                        Ok(m) => (m.symbol, m.decimals),
                        Err(_) => ("UNKNOWN".into(), 18),
                    }
                } else {
                    (native_symbol(chain).into(), 18)
                };
                let expected_contract = payment.expected_asset.token_contract.as_deref();
                let contract_matches = match (expected_contract, transfer.token_contract.as_deref())
                {
                    (None, None) => true,
                    (Some(a), Some(b)) => a.eq_ignore_ascii_case(b),
                    _ => false,
                };
                let symbol_matches = symbol.eq_ignore_ascii_case(&payment.expected_asset.symbol);
                let chain_matches = chain == &payment.expected_chain;
                let classification = if chain_matches && contract_matches && symbol_matches {
                    "EXPECTED_ASSET"
                } else if transfer.token_contract.is_some() {
                    "WRONG_ASSET"
                } else {
                    "NATIVE_TRANSFER"
                };
                let dep = StoredDeposit {
                    chain: chain.clone(),
                    tx_hash: transfer.tx_hash.clone(),
                    log_index: transfer
                        .log_index
                        .map(|v| i64::try_from(v).unwrap_or(i64::MAX)),
                    from_address: transfer.from,
                    to_address: transfer.to,
                    asset_symbol: symbol,
                    token_contract: transfer.token_contract,
                    asset_decimals: decimals,
                    amount: transfer.amount,
                    classification: classification.into(),
                    confirmation_status: if conf.final_enough {
                        "FINAL".into()
                    } else {
                        "CONFIRMING".into()
                    },
                    confirmations: conf.confirmations,
                };
                saw_new |= state
                    .store
                    .record_verified_deposit(
                        payment.id,
                        &dep,
                        transfer.block_number,
                        &transfer.block_hash,
                    )
                    .await?;
            }
            state
                .store
                .set_monitor_cursor(payment.id, chain, to, None)
                .await?;
            if saw_new {
                payment = state.store.get_payment_by_id(payment.id).await?;
                move_to_detected(state, &mut payment).await?;
            }
            let deposits = state.store.payment_deposits(payment.id).await?;
            if deposits.is_empty() {
                continue;
            }
            payment = state.store.get_payment_by_id(payment.id).await?;
            let active_exists = deposits.iter().any(|d| d.confirmation_status != "ORPHANED");
            if active_exists
                && matches!(
                    payment.state,
                    PaymentState::Waiting | PaymentState::PartiallyPaid | PaymentState::WrongAsset
                )
            {
                move_to_detected(state, &mut payment).await?;
            }
            let expected_exists = deposits.iter().any(|d| {
                d.classification == "EXPECTED_ASSET" && d.confirmation_status != "ORPHANED"
            });
            if expected_exists && payment.state == PaymentState::Detected {
                state
                    .store
                    .set_payment_state(
                        payment.id,
                        PaymentState::Confirming,
                        "deposit_verification_started",
                        Some(chain),
                        None,
                    )
                    .await?;
                payment = state.store.get_payment_by_id(payment.id).await?;
            }
            let observed = deposits
                .iter()
                .filter(|d| d.confirmation_status != "ORPHANED")
                .map(|d| ObservedDeposit {
                    chain: d.chain.to_string(),
                    tx_hash: d.tx_hash.clone(),
                    asset_symbol: d.asset_symbol.clone(),
                    token_contract: d.token_contract.clone(),
                    amount: d.amount.clone(),
                    final_enough: d.confirmation_status == "FINAL",
                })
                .collect();
            let result = reconcile(&ReconciliationInput {
                expected_chain: payment.expected_chain.to_string(),
                expected_asset_symbol: payment.expected_asset.symbol.clone(),
                expected_token_contract: payment.expected_asset.token_contract.clone(),
                expected_amount: payment.expected_amount.clone(),
                current_state: payment.state,
                overpayment_policy: payment.overpayment_policy.clone(),
                deposits: observed,
            })?;
            if result.next_state != payment.state {
                if payment.state.can_transition_to(result.next_state) {
                    state
                        .store
                        .set_payment_state(
                            payment.id,
                            result.next_state,
                            &result.reason_code,
                            Some(chain),
                            None,
                        )
                        .await?;
                    emit_payment_event(state, &payment, result.next_state, &result.reason_code)
                        .await?;
                }
            }
        }
    }
    Ok(())
}

pub(crate) async fn process_alchemy_webhook(
    state: &AppState,
    payload: &Value,
) -> anyhow::Result<()> {
    let network = payload
        .pointer("/event/network")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("Alchemy payload has no event network"))?;
    let chain = match network.to_ascii_uppercase().as_str() {
        "BASE_SEPOLIA" => ChainKey::Custom("base_sepolia".into()),
        "ETH_SEPOLIA" => ChainKey::Custom("ethereum_sepolia".into()),
        "ARB_SEPOLIA" => ChainKey::Custom("arbitrum_sepolia".into()),
        "OPT_SEPOLIA" => ChainKey::Custom("optimism_sepolia".into()),
        "MATIC_AMOY" | "POLYGON_AMOY" => ChainKey::Custom("polygon_amoy".into()),
        other => return Err(anyhow::anyhow!("unsupported Alchemy network: {other}")),
    };
    if !state.chains.contains_key(&chain) {
        return Err(anyhow::anyhow!(
            "Alchemy network is not configured: {chain}"
        ));
    }

    let activities = payload
        .pointer("/event/activity")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow::anyhow!("Alchemy payload has no activity array"))?;
    let mut payments = Vec::new();
    for activity in activities {
        let Some(address) = activity.get("toAddress").and_then(Value::as_str) else {
            continue;
        };
        let Some(block_number) = activity
            .get("blockNum")
            .and_then(Value::as_str)
            .and_then(parse_hex_u64)
        else {
            continue;
        };
        let rows = sqlx::query(
            "SELECT DISTINCT p.id FROM payments p JOIN checkout_addresses c ON c.payment_id=p.id WHERE c.chain=$1 AND lower(c.address)=lower($2) AND p.state IN ('WAITING','DETECTED','CONFIRMING','PARTIALLY_PAID','OVERPAID','WRONG_ASSET')",
        )
        .bind(chain.to_string())
        .bind(address)
        .fetch_all(state.store.pool())
        .await?;
        let tx_hash = activity
            .get("hash")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow::anyhow!("Alchemy activity has no transaction hash"))?;
        let from = activity
            .get("fromAddress")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        let symbol = activity
            .get("asset")
            .and_then(Value::as_str)
            .unwrap_or("UNKNOWN");
        let raw = activity
            .pointer("/rawContract/rawValue")
            .and_then(Value::as_str);
        let decimals = activity
            .pointer("/rawContract/decimals")
            .and_then(|value| {
                value
                    .as_u64()
                    .or_else(|| value.as_str().and_then(parse_hex_u64))
            })
            .unwrap_or(18)
            .min(255) as u8;
        let token_contract = activity
            .pointer("/rawContract/address")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .map(str::to_owned);
        let amount = if let Some(raw) = raw {
            AtomicAmount::from_hex_quantity(raw)?
        } else {
            AtomicAmount::from_decimal(
                activity
                    .get("value")
                    .and_then(Value::as_f64)
                    .unwrap_or_default()
                    .to_string()
                    .as_str(),
                decimals,
            )?
        };
        for row in rows {
            let payment_id = flowpay_domain::PaymentId(row.try_get("id")?);
            let mut payment = state.store.get_payment_by_id(payment_id).await?;
            let contract_matches = match (
                payment.expected_asset.token_contract.as_deref(),
                token_contract.as_deref(),
            ) {
                (None, None) => true,
                (Some(a), Some(b)) => a.eq_ignore_ascii_case(b),
                _ => false,
            };
            let classification = if chain == payment.expected_chain
                && symbol.eq_ignore_ascii_case(&payment.expected_asset.symbol)
                && contract_matches
            {
                "EXPECTED_ASSET"
            } else if token_contract.is_some() {
                "WRONG_ASSET"
            } else {
                "NATIVE_TRANSFER"
            };
            let deposit = StoredDeposit {
                chain: chain.clone(),
                tx_hash: tx_hash.to_owned(),
                log_index: None,
                from_address: from.to_owned(),
                to_address: address.to_owned(),
                asset_symbol: symbol.to_owned(),
                token_contract: token_contract.clone(),
                asset_decimals: decimals,
                amount: amount.clone(),
                classification: classification.into(),
                confirmation_status: "FINAL".into(),
                confirmations: 0,
            };
            state
                .store
                .record_verified_deposit(
                    payment_id,
                    &deposit,
                    block_number,
                    &format!("alchemy-notify:{network}:{block_number}"),
                )
                .await?;
            move_to_detected(state, &mut payment).await?;
            if !payments.contains(&payment_id) {
                payments.push(payment_id);
            }
        }
    }
    if payments.is_empty() {
        return Ok(());
    }

    for payment_id in payments {
        reconcile_webhook_payment(state, payment_id, &chain).await?;
    }
    Ok(())
}

async fn reconcile_webhook_payment(
    state: &AppState,
    payment_id: flowpay_domain::PaymentId,
    chain: &ChainKey,
) -> anyhow::Result<()> {
    let mut payment = state.store.get_payment_by_id(payment_id).await?;
    let deposits = state.store.payment_deposits(payment_id).await?;
    if deposits
        .iter()
        .any(|deposit| deposit.classification == "EXPECTED_ASSET")
        && payment.state == PaymentState::Detected
    {
        state
            .store
            .set_payment_state(
                payment.id,
                PaymentState::Confirming,
                "alchemy_webhook_received",
                Some(chain),
                None,
            )
            .await?;
        payment = state.store.get_payment_by_id(payment.id).await?;
    }
    let observed = deposits
        .into_iter()
        .map(|deposit| ObservedDeposit {
            chain: deposit.chain.to_string(),
            tx_hash: deposit.tx_hash,
            asset_symbol: deposit.asset_symbol,
            token_contract: deposit.token_contract,
            amount: deposit.amount,
            final_enough: true,
        })
        .collect();
    let result = reconcile(&ReconciliationInput {
        expected_chain: payment.expected_chain.to_string(),
        expected_asset_symbol: payment.expected_asset.symbol.clone(),
        expected_token_contract: payment.expected_asset.token_contract.clone(),
        expected_amount: payment.expected_amount.clone(),
        current_state: payment.state,
        overpayment_policy: payment.overpayment_policy.clone(),
        deposits: observed,
    })?;
    if result.next_state != payment.state && payment.state.can_transition_to(result.next_state) {
        state
            .store
            .set_payment_state(
                payment.id,
                result.next_state,
                &result.reason_code,
                Some(chain),
                None,
            )
            .await?;
        emit_payment_event(state, &payment, result.next_state, &result.reason_code).await?;
    }
    Ok(())
}

fn parse_hex_u64(value: &str) -> Option<u64> {
    u64::from_str_radix(value.strip_prefix("0x").unwrap_or(value), 16).ok()
}

async fn move_to_detected(state: &AppState, payment: &mut Payment) -> anyhow::Result<()> {
    match payment.state {
        PaymentState::Waiting | PaymentState::PartiallyPaid => {
            state
                .store
                .set_payment_state(
                    payment.id,
                    PaymentState::Detected,
                    "deposit_detected",
                    Some(&payment.expected_chain),
                    None,
                )
                .await?;
            payment.state = PaymentState::Detected;
        }
        PaymentState::WrongAsset => {
            state
                .store
                .set_payment_state(
                    payment.id,
                    PaymentState::Waiting,
                    "new_deposit_after_wrong_asset",
                    Some(&payment.expected_chain),
                    None,
                )
                .await?;
            state
                .store
                .set_payment_state(
                    payment.id,
                    PaymentState::Detected,
                    "deposit_detected",
                    Some(&payment.expected_chain),
                    None,
                )
                .await?;
            payment.state = PaymentState::Detected;
        }
        _ => {}
    }
    Ok(())
}

async fn settlement_tick(state: &AppState) -> anyhow::Result<()> {
    let Some(holder) = try_acquire_service_lease(state, "settlement-coordinator", 120).await?
    else {
        return Ok(());
    };
    let heartbeat =
        spawn_lease_heartbeat(state.clone(), "settlement-coordinator", holder.clone(), 120);
    let result = settlement_tick_inner(state).await;
    heartbeat.abort();
    release_service_lease(state, "settlement-coordinator", &holder).await;
    result
}

async fn settlement_tick_inner(state: &AppState) -> anyhow::Result<()> {
    let rows=sqlx::query("SELECT id FROM payments WHERE state IN ('CONFIRMED','OVERPAID') ORDER BY updated_at LIMIT 10")
        .fetch_all(state.store.pool()).await?;

    for row in rows {
        let payment = state
            .store
            .get_payment_by_id(flowpay_domain::PaymentId(row.try_get("id")?))
            .await?;
        let Some(runtime) = state.chains.get(&payment.expected_chain) else {
            continue;
        };

        // Settlement is consequential. Re-check every persisted deposit immediately before
        // simulating/signing so a disappeared log or block-hash change cannot be settled.
        match revalidate_persisted_deposits(state, &payment, &payment.expected_chain, runtime)
            .await?
        {
            DepositRevalidation::Stable => {}
            DepositRevalidation::Reorged => {
                rewind_payment_after_reorg(state, &payment).await?;
                warn!(payment_id=%payment.id.0,"settlement withheld because a deposit was orphaned");
                continue;
            }
            DepositRevalidation::Unavailable => {
                warn!(payment_id=%payment.id.0,"settlement withheld because deposit canonicality is temporarily unavailable");
                continue;
            }
        }
        let has_exception_deposit: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM deposits WHERE payment_id=$1 AND confirmation_status<>'ORPHANED' AND classification<>'EXPECTED_ASSET')",
        )
        .bind(payment.id.0)
        .fetch_one(state.store.pool())
        .await?;
        if has_exception_deposit {
            warn!(payment_id=%payment.id.0,"settlement withheld because an active wrong-asset or wrong-chain deposit requires claim review");
            continue;
        }

        let destination = state
            .store
            .merchant_settlement_address(payment.merchant_id)
            .await?;
        let salt = state
            .store
            .checkout_salt(payment.id, &payment.expected_chain)
            .await?;
        let canonical_amount: String = sqlx::query_scalar(
            "SELECT COALESCE(sum(amount_atomic), 0)::text FROM deposits WHERE payment_id=$1 AND chain=$2 AND classification='EXPECTED_ASSET' AND confirmation_status='FINAL' AND upper(asset_symbol)=upper($3) AND (($4::text IS NULL AND token_contract IS NULL) OR lower(token_contract)=lower($4))",
        )
        .bind(payment.id.0)
        .bind(payment.expected_chain.to_string())
        .bind(&payment.expected_asset.symbol)
        .bind(&payment.expected_asset.token_contract)
        .fetch_one(state.store.pool())
        .await?;
        let canonical_amount = canonical_amount.parse::<AtomicAmount>()?;
        if canonical_amount < payment.expected_amount {
            warn!(payment_id=%payment.id.0, canonical_amount=%canonical_amount, expected_amount=%payment.expected_amount, "settlement withheld because canonical deposits do not cover expected amount");
            continue;
        }
        let (tx, class) = if let Some(token) = payment.expected_asset.token_contract.as_deref() {
            (
                build_factory_erc20_sweep(
                    payment.expected_chain.clone(),
                    salt,
                    &runtime.factory,
                    token,
                    &destination,
                    &canonical_amount,
                    "SETTLE_ERC20",
                )?,
                TransactionClass::SettleErc20,
            )
        } else {
            (
                build_factory_native_sweep(
                    payment.expected_chain.clone(),
                    salt,
                    &runtime.factory,
                    &destination,
                    &canonical_amount,
                    "SETTLE_NATIVE",
                )?,
                TransactionClass::SettleNative,
            )
        };

        let simulation = runtime.adapter.simulate(&tx).await?;
        if !simulation.success {
            state
                .store
                .set_payment_state(
                    payment.id,
                    PaymentState::Failed,
                    "settlement_simulation_failed",
                    Some(&payment.expected_chain),
                    None,
                )
                .await?;
            continue;
        }

        state
            .store
            .set_payment_state(
                payment.id,
                PaymentState::Settling,
                "settlement_simulation_succeeded",
                Some(&payment.expected_chain),
                None,
            )
            .await?;
        let policy = SignerPolicy {
            allowed_classes: BTreeSet::from([class.clone()]),
            factory_address: runtime.factory.clone(),
        };
        let request = SettlementSignerRequest {
            payment_id: payment.id,
            transaction_class: class,
            transaction: tx,
            expected_factory: runtime.factory.clone(),
            settlement_destination: destination.clone(),
            configured_merchant_destination: destination.clone(),
        };
        let submission = if let Some(private_key) = state.config.operator_private_key.as_deref() {
            TestnetKeySigner::from_private_key(policy, &runtime.rpc_url, private_key)?
                .submit_settlement(&request)
                .await
        } else {
            DevUnlockedSigner::new(policy, &runtime.rpc_url, &state.config.operator_address)
                .submit_settlement(&request)
                .await
        };
        let tx_hash = match submission {
            Ok(v) => v,
            Err(e) => {
                state
                    .store
                    .set_payment_state(
                        payment.id,
                        PaymentState::Failed,
                        "settlement_signer_rejected",
                        Some(&payment.expected_chain),
                        None,
                    )
                    .await?;
                warn!(payment_id=%payment.id.0,error=%e,"settlement rejected");
                continue;
            }
        };

        sqlx::query("INSERT INTO settlements(payment_id,chain,asset_symbol,token_contract,amount_atomic,destination,state,tx_hash,simulation_result) VALUES($1,$2,$3,$4,$5::numeric,$6,'SUBMITTED',$7,$8) ON CONFLICT(payment_id) DO UPDATE SET state='SUBMITTED',tx_hash=EXCLUDED.tx_hash,simulation_result=EXCLUDED.simulation_result,updated_at=now()")
            .bind(payment.id.0)
            .bind(payment.expected_chain.to_string())
            .bind(&payment.expected_asset.symbol)
            .bind(&payment.expected_asset.token_contract)
            .bind(canonical_amount.to_string())
            .bind(&destination)
            .bind(&tx_hash)
            .bind(json!({"success":true,"gas_estimate":simulation.gas_estimate}))
            .execute(state.store.pool()).await?;

        let mut verified = false;
        for _ in 0..15 {
            match runtime.adapter.receipt(&tx_hash).await {
                Ok(r) if r.success => {
                    let remaining =
                        if let Some(token) = payment.expected_asset.token_contract.as_deref() {
                            runtime
                                .adapter
                                .token_balance(token, &payment.checkout_address.value)
                                .await?
                        } else {
                            runtime
                                .adapter
                                .native_balance(&payment.checkout_address.value)
                                .await?
                        };
                    if remaining.is_zero() {
                        verified = true;
                        break;
                    }
                }
                Ok(_) => break,
                Err(_) => tokio::time::sleep(StdDuration::from_millis(500)).await,
            }
        }

        if verified {
            sqlx::query(
                "UPDATE settlements SET state='CONFIRMED',updated_at=now() WHERE payment_id=$1",
            )
            .bind(payment.id.0)
            .execute(state.store.pool())
            .await?;
            state
                .store
                .set_payment_state(
                    payment.id,
                    PaymentState::Completed,
                    "settlement_verified",
                    Some(&payment.expected_chain),
                    Some(&tx_hash),
                )
                .await?;
            emit_payment_event(
                state,
                &payment,
                PaymentState::Completed,
                "settlement_verified",
            )
            .await?;
            info!(payment_id=%payment.id.0,tx_hash=%tx_hash,"payment completed");
        } else {
            state
                .store
                .set_payment_state(
                    payment.id,
                    PaymentState::Failed,
                    "settlement_verification_failed",
                    Some(&payment.expected_chain),
                    Some(&tx_hash),
                )
                .await?;
        }
    }
    Ok(())
}

async fn webhook_tick(state: &AppState) -> anyhow::Result<()> {
    let Some(holder) = try_acquire_service_lease(state, "webhook-delivery-coordinator", 60).await?
    else {
        return Ok(());
    };
    let heartbeat = spawn_lease_heartbeat(
        state.clone(),
        "webhook-delivery-coordinator",
        holder.clone(),
        60,
    );
    let result = webhook_tick_inner(state).await;
    heartbeat.abort();
    release_service_lease(state, "webhook-delivery-coordinator", &holder).await;
    result
}

async fn webhook_tick_inner(state: &AppState) -> anyhow::Result<()> {
    let rows=sqlx::query(
        "SELECT d.id,d.attempt,e.id AS event_id,e.public_id,e.event_type,e.payload,w.id AS endpoint_id,w.url,w.signing_secret_ciphertext FROM webhook_deliveries d JOIN webhook_events e ON e.id=d.webhook_event_id JOIN webhook_endpoints w ON w.id=d.webhook_endpoint_id WHERE d.status IN ('PENDING','RETRY') AND d.scheduled_at<=now() ORDER BY d.scheduled_at LIMIT 20"
    ).fetch_all(state.store.pool()).await?;

    for row in rows {
        let delivery_id: uuid::Uuid = row.try_get("id")?;
        let attempt: i32 = row.try_get("attempt")?;
        let url: String = row.try_get("url")?;
        let event_public_id: String = row.try_get("public_id")?;
        let sealed: Vec<u8> = row.try_get("signing_secret_ciphertext")?;
        let secret =
            flowpay_webhooks::decrypt_secret(&state.config.webhook_encryption_key, &sealed)?;
        let payload: serde_json::Value = row.try_get("payload")?;
        let body = serde_json::to_vec(&payload)?;
        let now = OffsetDateTime::now_utc();
        let signature = flowpay_webhooks::sign(&secret, now.unix_timestamp(), &body);
        let response = state
            .http
            .post(&url)
            .timeout(StdDuration::from_secs(10))
            .header("content-type", "application/json")
            .header("flowpay-signature", signature)
            .header("flowpay-event-id", &event_public_id)
            .body(body)
            .send()
            .await;

        match response {
            Ok(resp) if resp.status().is_success() => {
                sqlx::query("UPDATE webhook_deliveries SET status='DELIVERED',attempted_at=now(),response_status=$2 WHERE id=$1")
                    .bind(delivery_id)
                    .bind(i32::from(resp.status().as_u16()))
                    .execute(state.store.pool()).await?;
            }
            other => {
                let next_attempt = attempt + 1;
                if next_attempt > 8 {
                    sqlx::query("UPDATE webhook_deliveries SET status='DEAD',attempted_at=now(),error_code='DELIVERY_FAILED' WHERE id=$1")
                        .bind(delivery_id).execute(state.store.pool()).await?;
                } else {
                    let delay =
                        flowpay_webhooks::retry_delay(u32::try_from(next_attempt).unwrap_or(8));
                    let scheduled_at = OffsetDateTime::now_utc() + delay;
                    let event_id: uuid::Uuid = row.try_get("event_id")?;
                    let endpoint_id: uuid::Uuid = row.try_get("endpoint_id")?;
                    let mut tx = state.store.pool().begin().await?;
                    sqlx::query("UPDATE webhook_deliveries SET status='DEAD',attempted_at=now(),error_code='DELIVERY_FAILED' WHERE id=$1")
                        .bind(delivery_id).execute(&mut *tx).await?;
                    sqlx::query("INSERT INTO webhook_deliveries(webhook_event_id,webhook_endpoint_id,attempt,status,scheduled_at,error_code) VALUES($1,$2,$3,'RETRY',$4,'DELIVERY_FAILED')")
                        .bind(event_id).bind(endpoint_id).bind(next_attempt).bind(scheduled_at).execute(&mut *tx).await?;
                    enqueue_command_at_tx(
                        &mut tx,
                        "webhook.retry",
                        "webhook.retry",
                        "WEBHOOK_EVENT",
                        &event_public_id,
                        json!({"event_id":&event_public_id,"attempt":next_attempt}),
                        None,
                        None,
                        scheduled_at,
                    )
                    .await?;
                    tx.commit().await?;
                }
                if let Err(error) = other {
                    warn!(url=%url,error=%error,"webhook delivery failed");
                }
            }
        }
    }
    Ok(())
}

async fn emit_payment_event(
    state: &AppState,
    payment: &Payment,
    state_to: PaymentState,
    reason: &str,
) -> anyhow::Result<()> {
    let event_type = match state_to {
        PaymentState::Detected => Some("payment.detected"),
        PaymentState::PartiallyPaid => Some("payment.partially_paid"),
        PaymentState::Confirmed | PaymentState::Overpaid => Some("payment.confirmed"),
        PaymentState::Completed => Some("payment.completed"),
        PaymentState::Failed => Some("payment.failed"),
        _ => None,
    };
    if let Some(event_type) = event_type {
        state
            .store
            .enqueue_merchant_webhook_event(
                payment.merchant_id,
                event_type,
                "PAYMENT",
                &payment.public_id,
                json!({"id":&payment.public_id,"status":state_to.as_str(),"reason":reason}),
            )
            .await?;
    }
    Ok(())
}
fn native_symbol(chain: &ChainKey) -> &'static str {
    match chain {
        ChainKey::Base => "ETH",
        ChainKey::Bsc => "BNB",
        ChainKey::Custom(value)
            if matches!(
                value.as_str(),
                "base_sepolia" | "ethereum_sepolia" | "arbitrum_sepolia" | "optimism_sepolia"
            ) =>
        {
            "ETH"
        }
        ChainKey::Custom(value) if value == "bsc_testnet" => "BNB",
        _ => "NATIVE",
    }
}

async fn agent_tick(state: &AppState) -> anyhow::Result<()> {
    let Some(holder) = try_acquire_service_lease(state, "agent-recovery-coordinator", 180).await?
    else {
        return Ok(());
    };
    let heartbeat = spawn_lease_heartbeat(
        state.clone(),
        "agent-recovery-coordinator",
        holder.clone(),
        180,
    );
    let result = agent_tick_inner(state).await;
    heartbeat.abort();
    release_service_lease(state, "agent-recovery-coordinator", &holder).await;
    result
}

async fn agent_tick_inner(state: &AppState) -> anyhow::Result<()> {
    use flowpay_agent::model::{
        ModelDrivenAgent, ModelProtocol, OpenAiResponsesClient, OpenAiResponsesConfig,
    };
    use flowpay_agent::{
        AgentContext, AgentRunResult, AgentRunStatus, ControlledRecoveryTools, SafetyFirstAgent,
    };

    // Baseline intentionally leaves exception claims for manual review. The deterministic
    // payment engine still runs in its own worker; only exception investigation is disabled.
    if state.config.agent_mode.eq_ignore_ascii_case("baseline") {
        return Ok(());
    }

    sqlx::query("UPDATE agent_runs SET status='FAILED',final_disposition='RETRY',completed_at=now() WHERE status='RUNNING' AND started_at < now()-interval '3 minutes'")
        .execute(state.store.pool())
        .await?;

    let model_requested = state.config.agent_mode.eq_ignore_ascii_case("model");
    // Config::from_env fail-closes any provider other than Ollama. A model-mode
    // claim is therefore either investigated by Ollama or left for bounded retry
    // and escalation; it is never silently routed to another provider.
    let model_available = model_requested;

    if !model_requested || model_available {
        let rows=sqlx::query("SELECT c.id,c.payment_id,c.merchant_id FROM claims c WHERE c.state='INVESTIGATING' AND NOT EXISTS (SELECT 1 FROM agent_runs r WHERE r.claim_id=c.id AND r.status='RUNNING') ORDER BY c.updated_at LIMIT 5")
            .fetch_all(state.store.pool()).await?;

        for row in rows {
            let claim_id = flowpay_domain::ClaimId(row.try_get("id")?);
            let payment_id = flowpay_domain::PaymentId(row.try_get("payment_id")?);
            let merchant_id: uuid::Uuid = row.try_get("merchant_id")?;

            if model_requested {
                let failed_runs:i64=sqlx::query_scalar("SELECT count(*)::bigint FROM agent_runs WHERE claim_id=$1 AND orchestration_mode='MODEL_TOOL_USE' AND status='FAILED'")
                    .bind(claim_id.0).fetch_one(state.store.pool()).await?;
                if failed_runs >= i64::try_from(state.config.agent_retry_budget).unwrap_or(3) {
                    warn!(claim_id=%claim_id.0,failed_runs,retry_budget=state.config.agent_retry_budget,"model investigation retry budget exhausted; escalating safely");
                    let _ = state
                        .store
                        .set_claim_state(
                            claim_id,
                            flowpay_domain::ClaimState::Escalated,
                            "agent_retry_budget_exhausted",
                            "AGENT",
                        )
                        .await;
                    let payment = state.store.get_payment_by_id(payment_id).await?;
                    if payment.state.can_transition_to(PaymentState::Escalated) {
                        let _ = state
                            .store
                            .set_payment_state(
                                payment_id,
                                PaymentState::Escalated,
                                "agent_retry_budget_exhausted",
                                None,
                                None,
                            )
                            .await;
                    }
                    continue;
                }
            }

            let run_id = uuid::Uuid::now_v7();
            let public_id = format!("arun_{}", run_id.simple());
            let (agent_version, provider, model_name, mode) = if model_requested {
                (
                    "model-investigator-v1",
                    Some(state.config.model_provider.as_str()),
                    Some(state.config.openai_model.as_str()),
                    "MODEL_TOOL_USE",
                )
            } else {
                ("safety-first-v1", None, None, "DETERMINISTIC")
            };
            sqlx::query("INSERT INTO agent_runs(id,public_id,claim_id,payment_id,agent_version,policy_version,status,model_provider,model_name,orchestration_mode) VALUES($1,$2,$3,$4,$5,'demo-v1','RUNNING',$6,$7,$8)")
                .bind(run_id).bind(&public_id).bind(claim_id.0).bind(payment_id.0).bind(agent_version).bind(provider).bind(model_name).bind(mode)
                .execute(state.store.pool()).await?;

            let ctx = AgentContext {
                agent_run_id: public_id.clone(),
                merchant_id: merchant_id.to_string(),
                claim_id,
                payment_id,
            };
            let result: Result<AgentRunResult, flowpay_agent::ToolError> = if model_requested {
                let mut cfg =
                    OpenAiResponsesConfig::new("ollama".into(), state.config.openai_model.clone());
                cfg.endpoint = state.config.openai_endpoint.clone();
                cfg.max_steps = state.config.agent_max_steps.clamp(4, 24);
                cfg.protocol = ModelProtocol::OllamaChat;
                let agent = ModelDrivenAgent::new(
                    DatabaseAgentTools::new(state.clone()),
                    OpenAiResponsesClient::new(cfg),
                );
                agent.investigate(&ctx).await
            } else {
                SafetyFirstAgent::new(DatabaseAgentTools::new(state.clone()))
                    .investigate(&ctx)
                    .await
            };

            match result {
                Ok(result) => {
                    persist_trajectory(state, run_id, &result.trajectory).await?;
                    let (disposition, status) = match result.status {
                        AgentRunStatus::RecoverableAwaitingApproval
                        | AgentRunStatus::RecoverableAwaitingFunding => {
                            ("RECOVERABLE", "COMPLETED")
                        }
                        AgentRunStatus::NeedsMoreEvidence => ("NEEDS_MORE_EVIDENCE", "COMPLETED"),
                        AgentRunStatus::NotRecoverable => ("NOT_RECOVERABLE", "COMPLETED"),
                        AgentRunStatus::Escalated => ("ESCALATE", "ESCALATED"),
                        AgentRunStatus::Recovered => ("RECOVERABLE", "COMPLETED"),
                    };
                    sqlx::query("UPDATE agent_runs SET status=$2,final_disposition=$3,completed_at=now() WHERE id=$1")
                        .bind(run_id).bind(status).bind(disposition).execute(state.store.pool()).await?;
                    match result.status {
                        AgentRunStatus::RecoverableAwaitingFunding => {
                            let _ = state
                                .store
                                .set_claim_state(
                                    claim_id,
                                    flowpay_domain::ClaimState::Recoverable,
                                    "recovery_waiting_for_test_gas",
                                    "AGENT",
                                )
                                .await;
                            let payment = state.store.get_payment_by_id(payment_id).await?;
                            if payment.state == PaymentState::ClaimPending {
                                let _ = state
                                    .store
                                    .set_payment_state(
                                        payment_id,
                                        PaymentState::RecoveryAvailable,
                                        "recovery_waiting_for_test_gas",
                                        None,
                                        None,
                                    )
                                    .await;
                            }
                        }
                        AgentRunStatus::NeedsMoreEvidence => {
                            let _ = state
                                .store
                                .set_claim_state(
                                    claim_id,
                                    flowpay_domain::ClaimState::NeedsMoreEvidence,
                                    "agent_requires_more_evidence",
                                    "AGENT",
                                )
                                .await;
                        }
                        AgentRunStatus::NotRecoverable => {
                            let _ = state
                                .store
                                .set_claim_state(
                                    claim_id,
                                    flowpay_domain::ClaimState::NotRecoverable,
                                    "agent_verified_not_recoverable",
                                    "AGENT",
                                )
                                .await;
                        }
                        AgentRunStatus::Escalated => {
                            let _ = state
                                .store
                                .set_claim_state(
                                    claim_id,
                                    flowpay_domain::ClaimState::Escalated,
                                    "agent_escalated",
                                    "AGENT",
                                )
                                .await;
                            let payment = state.store.get_payment_by_id(payment_id).await?;
                            if payment.state.can_transition_to(PaymentState::Escalated) {
                                let _ = state
                                    .store
                                    .set_payment_state(
                                        payment_id,
                                        PaymentState::Escalated,
                                        "agent_escalated",
                                        None,
                                        None,
                                    )
                                    .await;
                            }
                        }
                        _ => {}
                    }
                }
                Err(e) => {
                    warn!(claim_id=%claim_id.0,error=%e,"agent investigation failed");
                    let retryable = matches!(e, flowpay_agent::ToolError::Retryable(_));
                    sqlx::query("UPDATE agent_runs SET status=$2,final_disposition=CASE WHEN $2='ESCALATED' THEN 'ESCALATE' ELSE NULL END,completed_at=now() WHERE id=$1")
                        .bind(run_id).bind(if retryable{"FAILED"}else{"ESCALATED"}).execute(state.store.pool()).await?;
                    if !retryable {
                        let _ = state
                            .store
                            .set_claim_state(
                                claim_id,
                                flowpay_domain::ClaimState::Escalated,
                                "agent_tool_failure",
                                "AGENT",
                            )
                            .await;
                    }
                }
            }
        }
    }

    // Money-moving recovery resumption remains deterministic even when investigation used a model.
    let approvals=sqlx::query("SELECT a.id,a.recovery_plan_id,a.claim_id,c.payment_id,c.merchant_id FROM approvals a JOIN claims c ON c.id=a.claim_id WHERE a.status='APPROVED' AND a.expires_at>now() AND c.state='APPROVAL_PENDING' ORDER BY a.approved_at LIMIT 5")
        .fetch_all(state.store.pool()).await?;
    for row in approvals {
        let approval_id = flowpay_domain::ApprovalId(row.try_get("id")?);
        let plan_id = flowpay_domain::RecoveryPlanId(row.try_get("recovery_plan_id")?);
        let claim_id = flowpay_domain::ClaimId(row.try_get("claim_id")?);
        let payment_id = flowpay_domain::PaymentId(row.try_get("payment_id")?);
        let merchant_id: uuid::Uuid = row.try_get("merchant_id")?;
        let run_id = uuid::Uuid::now_v7();
        let public_id = format!("arun_{}", run_id.simple());
        sqlx::query("INSERT INTO agent_runs(id,public_id,claim_id,payment_id,agent_version,policy_version,status,orchestration_mode) VALUES($1,$2,$3,$4,'deterministic-recovery-resume-v1','demo-v1','RUNNING','DETERMINISTIC')")
            .bind(run_id).bind(&public_id).bind(claim_id.0).bind(payment_id.0).execute(state.store.pool()).await?;
        let ctx = AgentContext {
            agent_run_id: public_id,
            merchant_id: merchant_id.to_string(),
            claim_id,
            payment_id,
        };
        let agent = SafetyFirstAgent::new(DatabaseAgentTools::new(state.clone()));
        match agent
            .execute_after_approval(&ctx, plan_id, approval_id)
            .await
        {
            Ok(result) => {
                persist_trajectory(state, run_id, &result.trajectory).await?;
                let ok = matches!(result.status, AgentRunStatus::Recovered);
                sqlx::query("UPDATE agent_runs SET status=$2,final_disposition=$3,completed_at=now() WHERE id=$1")
                    .bind(run_id).bind(if ok{"COMPLETED"}else{"ESCALATED"}).bind(if ok{"RECOVERABLE"}else{"ESCALATE"}).execute(state.store.pool()).await?;
                if !ok {
                    let _ = state
                        .store
                        .set_claim_state(
                            claim_id,
                            flowpay_domain::ClaimState::Escalated,
                            "recovery_verification_failed",
                            "AGENT",
                        )
                        .await;
                    let payment = state.store.get_payment_by_id(payment_id).await?;
                    if payment.state.can_transition_to(PaymentState::Escalated) {
                        let _ = state
                            .store
                            .set_payment_state(
                                payment_id,
                                PaymentState::Escalated,
                                "recovery_verification_failed",
                                None,
                                result.recovery_transaction_hash.as_deref(),
                            )
                            .await;
                    }
                }
            }
            Err(e) => {
                warn!(claim_id=%claim_id.0,error=%e,"approved recovery execution failed");
                sqlx::query("UPDATE agent_runs SET status='ESCALATED',final_disposition='ESCALATE',completed_at=now() WHERE id=$1").bind(run_id).execute(state.store.pool()).await?;
                let _ = state
                    .store
                    .set_claim_state(
                        claim_id,
                        flowpay_domain::ClaimState::Escalated,
                        "recovery_execution_failure",
                        "AGENT",
                    )
                    .await;
            }
        }
    }

    // A transaction may be mined even if the process fails while persisting the
    // post-submit state. Reconcile every submitted execution from its receipt and
    // destination balance so restarts never strand a successful refund.
    let submitted = sqlx::query(
        "SELECT e.recovery_plan_id,e.tx_hash,r.claim_id,c.payment_id,c.merchant_id \
         FROM recovery_executions e \
         JOIN recovery_plans r ON r.id=e.recovery_plan_id \
         JOIN claims c ON c.id=r.claim_id \
         WHERE e.state='SUBMITTED' ORDER BY e.created_at LIMIT 10",
    )
    .fetch_all(state.store.pool())
    .await?;
    for row in submitted {
        let plan_id = flowpay_domain::RecoveryPlanId(row.try_get("recovery_plan_id")?);
        let claim_id = flowpay_domain::ClaimId(row.try_get("claim_id")?);
        let payment_id = flowpay_domain::PaymentId(row.try_get("payment_id")?);
        let merchant_id: uuid::Uuid = row.try_get("merchant_id")?;
        let tx_hash: String = row.try_get("tx_hash")?;
        let claim = state.store.get_claim_by_id(claim_id).await?;
        if claim.state == flowpay_domain::ClaimState::Escalated {
            state
                .store
                .set_claim_state(
                    claim_id,
                    flowpay_domain::ClaimState::RecoveryPending,
                    "submitted_recovery_reconciliation",
                    "SYSTEM",
                )
                .await?;
        }
        let payment = state.store.get_payment_by_id(payment_id).await?;
        if payment.state == PaymentState::Escalated {
            state
                .store
                .set_payment_state(
                    payment_id,
                    PaymentState::RecoveryPending,
                    "submitted_recovery_reconciliation",
                    None,
                    Some(&tx_hash),
                )
                .await?;
        }
        let ctx = AgentContext {
            agent_run_id: format!("reconcile_{}", plan_id.0.simple()),
            merchant_id: merchant_id.to_string(),
            claim_id,
            payment_id,
        };
        match DatabaseAgentTools::new(state.clone())
            .verify_recovery(&ctx, plan_id, &tx_hash)
            .await
        {
            Ok(true) => {
                info!(claim_id=%claim_id.0,transaction_hash=%tx_hash,"submitted recovery reconciled and verified")
            }
            Ok(false) => {
                warn!(claim_id=%claim_id.0,transaction_hash=%tx_hash,"submitted recovery receipt failed verification")
            }
            Err(e) => {
                warn!(claim_id=%claim_id.0,transaction_hash=%tx_hash,error=%e,"submitted recovery reconciliation deferred")
            }
        }
    }
    Ok(())
}

async fn try_acquire_service_lease(
    state: &AppState,
    key: &str,
    ttl_seconds: i64,
) -> anyhow::Result<Option<String>> {
    let holder = format!("worker-{}", uuid::Uuid::now_v7());
    let acquired:Option<String>=sqlx::query_scalar(
        "INSERT INTO service_leases(lease_key,holder_id,expires_at) VALUES($1,$2,now()+make_interval(secs => $3)) ON CONFLICT(lease_key) DO UPDATE SET holder_id=EXCLUDED.holder_id,expires_at=EXCLUDED.expires_at,updated_at=now() WHERE service_leases.expires_at<now() RETURNING holder_id"
    ).bind(key).bind(&holder).bind(ttl_seconds).fetch_optional(state.store.pool()).await?;
    Ok(acquired.map(|_| holder))
}

fn spawn_lease_heartbeat(
    state: AppState,
    key: &'static str,
    holder: String,
    ttl_seconds: i64,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let interval = (ttl_seconds.max(30) / 3) as u64;
        loop {
            tokio::time::sleep(StdDuration::from_secs(interval)).await;
            let result = sqlx::query(
                "UPDATE service_leases SET expires_at=now()+make_interval(secs => $3),updated_at=now() WHERE lease_key=$1 AND holder_id=$2",
            )
            .bind(key)
            .bind(&holder)
            .bind(ttl_seconds)
            .execute(state.store.pool())
            .await;
            match result {
                Ok(result) if result.rows_affected() == 1 => {}
                Ok(_) => {
                    warn!(lease_key=%key, "service lease heartbeat lost ownership");
                    break;
                }
                Err(error) => {
                    warn!(lease_key=%key, error=%error, "service lease heartbeat failed");
                }
            }
        }
    })
}

async fn release_service_lease(state: &AppState, key: &str, holder: &str) {
    if let Err(error) =
        sqlx::query("DELETE FROM service_leases WHERE lease_key=$1 AND holder_id=$2")
            .bind(key)
            .bind(holder)
            .execute(state.store.pool())
            .await
    {
        warn!(lease_key=%key,error=%error,"failed to release service lease; expiry will recover it");
    }
}

async fn persist_trajectory(
    state: &AppState,
    run_id: uuid::Uuid,
    steps: &[flowpay_agent::TrajectoryStep],
) -> anyhow::Result<()> {
    for step in steps {
        sqlx::query("INSERT INTO agent_tool_calls(agent_run_id,step_number,tool_name,input_redacted,output_redacted,status,completed_at) VALUES($1,$2,$3,$4,$5,'SUCCEEDED',now()) ON CONFLICT(agent_run_id,step_number) DO NOTHING").bind(run_id).bind(i32::try_from(step.sequence).unwrap_or(i32::MAX)).bind(&step.tool).bind(&step.input_summary).bind(&step.output_summary).execute(state.store.pool()).await?;
        sqlx::query("INSERT INTO agent_decisions(agent_run_id,step_number,decision_summary,verification_summary,chosen_tool) VALUES($1,$2,$3,$4,$5) ON CONFLICT(agent_run_id,step_number) DO NOTHING").bind(run_id).bind(i32::try_from(step.sequence).unwrap_or(i32::MAX)).bind(&step.rationale).bind(&step.verification).bind(&step.tool).execute(state.store.pool()).await?;
    }
    Ok(())
}

use crate::{AddressRef, AtomicAmount, ChainKey, MerchantId, PaymentId, PaymentState};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Asset {
    pub symbol: String,
    pub decimals: u8,
    pub token_contract: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum OverpaymentPolicy {
    AcceptAndRecord,
    RequireReview,
    RejectSettlement,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Payment {
    pub id: PaymentId,
    pub public_id: String,
    pub merchant_id: MerchantId,
    pub reference: Option<String>,
    pub expected_chain: ChainKey,
    pub expected_asset: Asset,
    pub expected_amount: AtomicAmount,
    pub checkout_address: AddressRef,
    pub state: PaymentState,
    pub required_confirmations: u64,
    pub overpayment_policy: OverpaymentPolicy,
    pub expires_at: OffsetDateTime,
}

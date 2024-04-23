pub use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

pub mod engine;
pub mod errors;
pub use engine::Engine;

pub type ClientId = u16;
pub type TransactionId = u32;
pub type Result<T> = std::result::Result<T, errors::Error>;

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum Event {
    #[serde(rename = "deposit")]
    Deposit { tx: TransactionId, amount: Decimal },
    #[serde(rename = "withdrawal")]
    Withdrawal { tx: TransactionId, amount: Decimal },
    #[serde(rename = "dispute")]
    Dispute { tx: TransactionId },
    #[serde(rename = "resolve")]
    Resolve { tx: TransactionId },
    #[serde(rename = "chargeback")]
    Chargeback { tx: TransactionId },
}

#[derive(Debug, Deserialize)]
pub struct Transaction {
    pub client: ClientId,
    #[serde(flatten)]
    pub event: Event,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct AccountInfo {
    pub client: ClientId,
    pub available: Decimal,
    pub held: Decimal,
    pub total: Decimal,
    pub locked: bool,
}

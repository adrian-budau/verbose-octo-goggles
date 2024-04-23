use std::fmt::{self, Display, Formatter};

use crate::{ClientId, TransactionId};

#[derive(Debug, PartialEq, Eq)]
pub enum ErrorType {
    ReusedTransactionId { tx: TransactionId },
    NegativeWithdrawal { tx: TransactionId },
    NegativeDeposit { tx: TransactionId },
    UnknownTransaction { tx: TransactionId },
    LockedAccount { client: ClientId },
    InsufficientFunds { client: ClientId, tx: TransactionId },
    UnknownTransactionForDispute { tx: TransactionId },
    TransactionDoesNotMatchClient { tx: TransactionId, client: ClientId },
    TransactionAlreadyUnderDispute { tx: TransactionId },
    TransactionNotUnderDispute { tx: TransactionId },
}

// wrapping error type to leave space for other (optional) data, such as backtrace
#[derive(Debug)]
pub struct Error {
    pub error_type: ErrorType,
}

impl From<ErrorType> for Error {
    fn from(error_type: ErrorType) -> Self {
        Self { error_type }
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self.error_type)
    }
}

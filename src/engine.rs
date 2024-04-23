use std::collections::HashMap;

use rust_decimal::Decimal;

use crate::{errors::ErrorType, AccountInfo, ClientId, Event, Result, Transaction, TransactionId};

#[derive(Default)]
pub struct Engine {
    state: HashMap<ClientId, ClientState>,
    funds_transactions: HashMap<TransactionId, TransactionInfo>,
    global_dispute: bool,
}

impl Engine {
    pub fn new() -> Self {
        Self {
            state: HashMap::new(),
            funds_transactions: HashMap::new(),
            global_dispute: false,
        }
    }

    pub fn set_global_dispute(&mut self, global_dispute: bool) {
        self.global_dispute = global_dispute;
    }

    /// transaction is moved here so that it won't accidently be double used
    pub fn handle(&mut self, transaction: Transaction) -> Result<()> {
        match transaction.event {
            Event::Deposit { tx, amount } if amount < Decimal::ZERO => {
                Err(ErrorType::NegativeDeposit { tx }.into())
            }
            Event::Deposit { tx, amount } => {
                let old = self
                    .funds_transactions
                    .insert(tx, TransactionInfo::new(transaction.client, amount));
                if old.is_some() {
                    return Err(ErrorType::ReusedTransactionId { tx }.into());
                }
                let account = self.state.entry(transaction.client).or_default();
                if account.locked {
                    return Err(ErrorType::LockedAccount {
                        client: transaction.client,
                    }
                    .into());
                }
                account.available += amount;
                Ok(())
            }
            Event::Withdrawal { tx, amount } if amount < Decimal::ZERO => {
                Err(ErrorType::NegativeWithdrawal { tx }.into())
            }
            Event::Withdrawal { tx, amount } => {
                let old = self
                    .funds_transactions
                    .insert(tx, TransactionInfo::new(transaction.client, -amount));
                if old.is_some() {
                    return Err(ErrorType::ReusedTransactionId { tx }.into());
                }
                let account = self.state.entry(transaction.client).or_default();
                if account.locked {
                    return Err(ErrorType::LockedAccount {
                        client: transaction.client,
                    }
                    .into());
                }
                if account.available < amount {
                    Err(ErrorType::InsufficientFunds {
                        client: transaction.client,
                        tx,
                    }
                    .into())
                } else {
                    account.available -= amount;
                    Ok(())
                }
            }
            Event::Dispute { tx } => {
                let info = self
                    .funds_transactions
                    .get_mut(&tx)
                    .ok_or(ErrorType::UnknownTransactionForDispute { tx })?;

                if info.client != transaction.client && !self.global_dispute {
                    return Err(ErrorType::TransactionDoesNotMatchClient {
                        tx,
                        client: transaction.client,
                    }
                    .into());
                }

                if info.status != Status::None {
                    return Err(ErrorType::TransactionAlreadyUnderDispute { tx })?;
                }
                info.status = Status::UnderDispute;
                if info.amount < Decimal::ZERO {
                    log::warn!("Disputing client {}'s withdrawal of {}(in transaction {}), it's likely the client has already taken the funds.", transaction.client, -info.amount, tx);
                }
                let account = self.state.entry(info.client).or_default();
                account.held += info.amount;
                account.available -= info.amount;
                Ok(())
            }
            Event::Resolve { tx } => {
                let info = self
                    .funds_transactions
                    .get_mut(&tx)
                    .ok_or(ErrorType::UnknownTransactionForDispute { tx })?;

                if info.client != transaction.client && !self.global_dispute {
                    return Err(ErrorType::TransactionDoesNotMatchClient {
                        tx,
                        client: transaction.client,
                    }
                    .into());
                }

                if info.status != Status::UnderDispute {
                    return Err(ErrorType::TransactionNotUnderDispute { tx })?;
                }
                info.status = Status::None;
                let account = self.state.entry(info.client).or_default();
                account.held -= info.amount;
                account.available += info.amount;
                Ok(())
            }
            Event::Chargeback { tx } => {
                let info = self
                    .funds_transactions
                    .get_mut(&tx)
                    .ok_or(ErrorType::UnknownTransactionForDispute { tx })?;

                if info.client != transaction.client && !self.global_dispute {
                    return Err(ErrorType::TransactionDoesNotMatchClient {
                        tx,
                        client: transaction.client,
                    }
                    .into());
                }

                if info.status != Status::UnderDispute {
                    return Err(ErrorType::TransactionNotUnderDispute { tx })?;
                }
                info.status = Status::Reversed;
                let account = self.state.entry(info.client).or_default();
                account.held -= info.amount;
                account.locked = true;
                Ok(())
            }
        }
    }

    pub fn account_info(&self, client: ClientId) -> AccountInfo {
        let Some(state) = self.state.get(&client) else {
            return AccountInfo {
                client,
                available: Decimal::ZERO,
                held: Decimal::ZERO,
                total: Decimal::ZERO,
                locked: false,
            };
        };
        AccountInfo {
            client,
            available: state.available,
            held: state.held,
            total: state.available + state.held,
            locked: state.locked,
        }
    }

    pub fn all_accounts(&self) -> impl Iterator<Item = AccountInfo> + '_ {
        self.state.iter().map(|(&client, state)| AccountInfo {
            client,
            available: state.available,
            held: state.held,
            total: state.available + state.held,
            locked: state.locked,
        })
    }
}

#[derive(PartialEq, Eq)]
enum Status {
    None,
    UnderDispute,
    Reversed,
}
struct TransactionInfo {
    client: ClientId,
    amount: Decimal,
    status: Status,
}

impl TransactionInfo {
    fn new(client: ClientId, amount: Decimal) -> Self {
        Self {
            client,
            amount,
            status: Status::None,
        }
    }
}
#[derive(Default)]
struct ClientState {
    available: Decimal,
    held: Decimal,
    locked: bool,
}

#[cfg(test)]
mod tests {
    use rust_decimal_macros::dec;

    use super::*;

    #[test]
    fn test_simple() {
        let mut engine = Engine::new();
        let client = 1;
        // add some funds
        engine
            .handle(Transaction {
                client,
                event: Event::Deposit {
                    tx: 1,
                    amount: dec!(1.2345),
                },
            })
            .unwrap();

        // withdraw too much
        assert_eq!(
            engine
                .handle(Transaction {
                    client,
                    event: Event::Withdrawal {
                        tx: 2,
                        amount: 2.into()
                    }
                })
                .unwrap_err()
                .error_type,
            ErrorType::InsufficientFunds { client, tx: 2 }
        );
        engine
            .handle(Transaction {
                client,
                event: Event::Withdrawal {
                    tx: 3,
                    amount: dec!(0.1234),
                },
            })
            .unwrap();

        let info: Vec<_> = engine.all_accounts().collect();
        assert_eq!(
            info,
            vec![AccountInfo {
                client,
                available: dec!(1.1111),
                held: Decimal::ZERO,
                total: dec!(1.1111),
                locked: false,
            }]
        )
    }

    struct Wrapper {
        engine: Engine,
        next_tx: TransactionId,
    }

    #[derive(Copy, Clone)]
    struct CommitedTransaction {
        client: ClientId,
        tx: TransactionId,
    }

    impl Wrapper {
        fn new() -> Self {
            Self {
                engine: Engine::new(),
                next_tx: 1,
            }
        }

        fn deposit(
            &mut self,
            client: ClientId,
            amount: impl Into<Decimal>,
        ) -> Result<CommitedTransaction> {
            let tx = self.next_tx;
            self.next_tx += 1;

            self.engine
                .handle(Transaction {
                    client,
                    event: Event::Deposit {
                        tx,
                        amount: amount.into(),
                    },
                })
                .map(|()| CommitedTransaction { client, tx })
        }

        fn withdraw(
            &mut self,
            client: ClientId,
            amount: impl Into<Decimal>,
        ) -> Result<CommitedTransaction> {
            let tx = self.next_tx;
            self.next_tx += 1;

            self.engine
                .handle(Transaction {
                    client,
                    event: Event::Withdrawal {
                        tx,
                        amount: amount.into(),
                    },
                })
                .map(|()| CommitedTransaction { client, tx })
        }

        fn dispute(&mut self, transaction: CommitedTransaction) -> Result<()> {
            self.engine.handle(Transaction {
                client: transaction.client,
                event: Event::Dispute { tx: transaction.tx },
            })
        }

        fn resolve(&mut self, transaction: CommitedTransaction) -> Result<()> {
            self.engine.handle(Transaction {
                client: transaction.client,
                event: Event::Resolve { tx: transaction.tx },
            })
        }

        fn chargeback(&mut self, transaction: CommitedTransaction) -> Result<()> {
            self.engine.handle(Transaction {
                client: transaction.client,
                event: Event::Chargeback { tx: transaction.tx },
            })
        }

        fn account_info(&self, client: ClientId) -> AccountInfo {
            self.engine.account_info(client)
        }
    }

    #[test]
    fn test_multiple() -> Result<()> {
        let mut engine = Wrapper::new();
        let client_a = 1;
        let client_b = 2;

        engine.deposit(client_a, 5)?;
        assert_eq!(engine.account_info(client_a).available, dec!(5));
        assert_eq!(engine.account_info(client_b).available, dec!(0));

        engine.deposit(client_b, 10)?;
        assert_eq!(engine.account_info(client_a).available, dec!(5));
        assert_eq!(engine.account_info(client_b).available, dec!(10));

        // this shouldn't work, even though b has enough, they have separate accounts
        assert!(engine.withdraw(client_a, 10).is_err());
        assert_eq!(engine.account_info(client_a).available, dec!(5));
        assert_eq!(engine.account_info(client_b).available, dec!(10));

        engine.withdraw(client_b, 10)?;
        engine.withdraw(client_a, 5)?;
        assert_eq!(engine.account_info(client_a).available, dec!(0));
        assert_eq!(engine.account_info(client_b).available, dec!(0));

        Ok(())
    }

    #[test]
    fn test_negative_balance() -> Result<()> {
        let mut engine = Wrapper::new();
        let client = 1;
        let deposit = engine.deposit(client, 10)?;
        engine.withdraw(client, 8)?;

        engine.dispute(deposit)?;
        assert!(engine.withdraw(client, 2).is_err()); // last of the funds, but they are held
        assert_eq!(engine.account_info(client).available, dec!(-8));
        assert_eq!(engine.account_info(client).held, dec!(10));

        // put some funds back
        engine.deposit(client, 5)?;
        engine.deposit(client, 7)?;
        assert_eq!(engine.account_info(client).available, dec!(4));
        assert_eq!(engine.account_info(client).held, dec!(10));

        engine.resolve(deposit)?;
        assert_eq!(engine.account_info(client).available, dec!(14));
        assert_eq!(engine.account_info(client).locked, false);
        Ok(())
    }

    #[test]
    fn locked_account() -> Result<()> {
        let mut engine = Wrapper::new();
        let client = 1;
        engine.deposit(client, 100)?;
        let fraudulent = engine.deposit(client, 100)?;
        let shady = engine.deposit(client, 100)?;

        assert_eq!(engine.account_info(client).available, dec!(300));
        assert_eq!(engine.account_info(client).locked, false);

        engine.withdraw(client, 50)?;
        engine.dispute(fraudulent)?;
        engine.chargeback(fraudulent)?;

        // Still has funds available
        assert_eq!(engine.account_info(client).available, dec!(150));
        assert_eq!(engine.account_info(client).locked, true);

        // Further deposits and/or withdrawals should fail
        assert!(engine.withdraw(client, 10).is_err());
        assert!(engine.deposit(client, 10000).is_err());

        // Other transactions can be disputed in the meantime
        engine.dispute(shady)?;
        assert_eq!(
            engine.account_info(client),
            AccountInfo {
                client,
                available: dec!(50),
                held: dec!(100),
                total: dec!(150),
                locked: true,
            }
        );
        Ok(())
    }

    #[test]
    fn globally_unique_transactions() {
        let mut engine = Engine::new();
        let client_a = 1;
        let client_b = 2;
        let tx = 1;
        engine
            .handle(Transaction {
                client: client_a,
                event: Event::Deposit {
                    tx,
                    amount: 10.into(),
                },
            })
            .unwrap();
        assert_eq!(
            engine
                .handle(Transaction {
                    client: client_b,
                    event: Event::Deposit {
                        tx,
                        amount: 10.into()
                    }
                })
                .unwrap_err()
                .error_type,
            ErrorType::ReusedTransactionId { tx }
        );
    }
}

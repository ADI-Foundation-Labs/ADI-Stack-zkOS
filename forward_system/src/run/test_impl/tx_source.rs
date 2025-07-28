use std::collections::VecDeque;

use crate::run::{NextTxResponse, TxSource};

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct TxListSource {
    pub transactions: VecDeque<Vec<u8>>,
}

impl TxSource for TxListSource {
    fn get_next_tx(&mut self) -> NextTxResponse {
        match self.transactions.pop_front() {
            Some(tx) => NextTxResponse::Tx(tx),
            None => NextTxResponse::SealBatch,
        }
    }
}

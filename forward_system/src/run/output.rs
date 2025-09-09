// Includes code adapted from https://github.com/bluealloy/revm/blob/fb80087996dfbd6c74eaf308538cfa707ecb763c/crates/context/interface/src/result.rs

use crate::run::result_keeper::ForwardRunningResultKeeper;
use crate::run::TxResultCallback;
pub use basic_bootloader::bootloader::block_header::BlockHeader;
use ruint::aliases::B160;
use zk_ee::common_structs::derive_flat_storage_key;
use zk_ee::system::errors::internal::InternalError;
use zk_ee::utils::Bytes32;
use zksync_os_interface::error::InvalidTransaction;
use zksync_os_interface::types::{ExecutionOutput, ExecutionResult, StorageWrite};

// Use interface type as the direct place-in, can be changed in the future.
pub use zksync_os_interface::types::TxOutput;

// Use interface type as the direct place-in, can be changed in the future.
pub use zksync_os_interface::types::BlockOutput;

pub type TxResult = Result<TxOutput, InvalidTransaction>;

trait StorageWriteExt {
    fn new(address: B160, key: Bytes32, value: Bytes32) -> StorageWrite;
}

impl StorageWriteExt for StorageWrite {
    fn new(address: B160, key: Bytes32, value: Bytes32) -> StorageWrite {
        let flat_key = derive_flat_storage_key(&address, &key);
        StorageWrite {
            key: flat_key.as_u8_array().into(),
            value: value.as_u8_array().into(),
            account: address.to_be_bytes().into(),
            account_key: key.as_u8_array().into(),
        }
    }
}

pub fn map_tx_results<TR: TxResultCallback, T: 'static + Sized>(
    result_keeper: &ForwardRunningResultKeeper<TR, T>,
) -> Vec<TxResult> {
    result_keeper
        .tx_results
        .iter()
        .enumerate()
        .map(|(tx_number, result)| {
            result.clone().map(|output| {
                let execution_result = if output.status {
                    ExecutionResult::Success(if output.contract_address.is_some() {
                        ExecutionOutput::Create(output.output, output.contract_address.unwrap())
                    } else {
                        ExecutionOutput::Call(output.output)
                    })
                } else {
                    ExecutionResult::Revert(output.output)
                };
                TxOutput {
                    gas_used: output.gas_used,
                    gas_refunded: output.gas_refunded,
                    native_used: output.native_used,

                    computational_native_used: output.computational_native_used,

                    pubdata_used: output.pubdata_used,
                    contract_address: output.contract_address,
                    logs: result_keeper
                        .events
                        .iter()
                        .filter_map(|e| {
                            if e.tx_number == tx_number as u32 {
                                Some(e.into())
                            } else {
                                None
                            }
                        })
                        .collect(),
                    l2_to_l1_logs: result_keeper
                        .logs
                        .iter()
                        .filter_map(|m| {
                            if m.tx_number == tx_number as u32 {
                                Some(m.into())
                            } else {
                                None
                            }
                        })
                        .collect(),
                    execution_result,
                    storage_writes: vec![],
                }
            })
        })
        .collect()
}

impl<TR: TxResultCallback, T: 'static + Sized> From<ForwardRunningResultKeeper<TR, T>>
    for BlockOutput
{
    fn from(value: ForwardRunningResultKeeper<TR, T>) -> Self {
        let ForwardRunningResultKeeper {
            block_header,
            events,
            logs,
            storage_writes,
            tx_results,
            new_preimages,
            pubdata,
            account_diffs,
            ..
        } = value;

        let mut block_computaional_native_used = 0;

        let tx_results = tx_results
            .into_iter()
            .enumerate()
            .map(|(tx_number, result)| {
                result.map(|output| {
                    let execution_result = if output.status {
                        ExecutionResult::Success(if output.contract_address.is_some() {
                            ExecutionOutput::Create(output.output, output.contract_address.unwrap())
                        } else {
                            ExecutionOutput::Call(output.output)
                        })
                    } else {
                        ExecutionResult::Revert(output.output)
                    };
                    block_computaional_native_used += output.computational_native_used;
                    TxOutput {
                        gas_used: output.gas_used,
                        gas_refunded: output.gas_refunded,
                        native_used: output.native_used,

                        computational_native_used: output.computational_native_used,

                        pubdata_used: output.pubdata_used,
                        contract_address: output.contract_address,
                        logs: events
                            .iter()
                            .filter_map(|e| {
                                if e.tx_number == tx_number as u32 {
                                    Some(e.into())
                                } else {
                                    None
                                }
                            })
                            .collect(),
                        l2_to_l1_logs: logs
                            .iter()
                            .filter_map(|m| {
                                if m.tx_number == tx_number as u32 {
                                    Some(m.into())
                                } else {
                                    None
                                }
                            })
                            .collect(),
                        execution_result,
                        storage_writes: vec![],
                    }
                })
            })
            .collect();

        let storage_writes = storage_writes
            .into_iter()
            .map(|(address, key, value)| StorageWrite::new(address, key, value))
            .collect();
        let account_diffs = account_diffs
            .into_iter()
            .map(|(address, (nonce, balance, code))| {
                (
                    address.to_be_bytes().into(),
                    (nonce, balance, code.as_u8_array().into()),
                )
            })
            .collect();
        let published_preimages = new_preimages
            .into_iter()
            .map(|(hash, data, typ)| (hash.as_u8_array().into(), data, typ))
            .collect();

        Self {
            header: block_header.unwrap().into(),
            tx_results,
            storage_writes,
            account_diffs,
            published_preimages,
            pubdata,
            computaional_native_used: block_computaional_native_used,
        }
    }
}

#[allow(dead_code)]
pub type BatchResult = Result<BlockOutput, InternalError>;

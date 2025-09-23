use crate::prestate::*;
use crate::receipts::TransactionReceipt;
use alloy::hex;
use alloy_rpc_types_eth::Withdrawal;
use rig::log::{error, info};
use rig::zksync_os_interface::types::{AccountDiff, BlockOutput, StorageWrite};
use ruint::aliases::{B160, B256, U256};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[allow(dead_code)]
pub enum PostCheckError {
    InvalidTx { id: TxId },
    TxShouldHaveFailed { id: TxId },
    IncorrectLogs { id: TxId },
    GasMismatch { id: TxId },
    Internal,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[allow(dead_code)]
pub enum TxId {
    Hash(String),
    Index(usize),
}

impl DiffTrace {
    fn collect_diffs(self, prestate_cache: &Cache, miner: B160) -> HashMap<B160, AccountState> {
        let mut updates: HashMap<B160, AccountState> = HashMap::new();
        self.result.iter().for_each(|item| {
            item.result.post.iter().for_each(|(address, account)| {
                if address.0 != miner {
                    let entry = updates.entry(address.0).or_default();
                    account
                        .balance
                        .into_iter()
                        .for_each(|bal| entry.balance = Some(bal));
                    account
                        .nonce
                        .into_iter()
                        .for_each(|x| entry.nonce = Some(x));
                    account
                        .code
                        .clone()
                        .into_iter()
                        .for_each(|x| entry.code = Some(x));

                    // Populate storage slot clears (slots present in pre but
                    // absent in post). Write 0 to them.
                    if let Some(pre_account) = item.result.pre.get(address) {
                        if let Some(pre_storage) = pre_account.storage.as_ref() {
                            let cleared_keys = pre_storage.keys().filter(|k| {
                                account
                                    .storage
                                    .as_ref()
                                    .is_none_or(|post_storage| !post_storage.contains_key(k))
                            });
                            let entry_storage = entry.storage.get_or_insert_default();
                            cleared_keys.into_iter().for_each(|key| {
                                entry_storage.insert(*key, B256::ZERO);
                            })
                        }
                    }

                    // Populate storage slot writes
                    if let Some(storage) = account.storage.as_ref() {
                        let entry_storage = entry.storage.get_or_insert_default();
                        storage.iter().for_each(|(key, value)| {
                            entry_storage.insert(*key, *value);
                        })
                    }
                }
            });
            // Add account clears
            item.result.pre.iter().for_each(|(address, _)| {
                if address.0 != miner && !updates.contains_key(&address.0) {
                    let acc = AccountState {
                        balance: Some(U256::ZERO),
                        ..Default::default()
                    };
                    updates.insert(address.0, acc);
                }
            })
        });

        // Filter out empty diffs
        // These can be empty because their value is the same as in the initial tree
        // or the post state was empty. Note that if the account was selfdestructed,
        // the address shouldn't be present in the post state. This is just a strange
        // case where the logs add an empty entry for accounts that haven't been
        // modified.

        updates.retain(|address, account| {
            if let Some(storage) = account.storage.as_mut() {
                storage.retain(|key, new_val| match prestate_cache.get_slot(address, key) {
                    None => *new_val != B256::ZERO,
                    Some(initial) => *new_val != initial,
                })
            }
            if account.storage.as_ref().is_some_and(|s| s.is_empty()) {
                account.storage = None
            }
            if account.balance == prestate_cache.get_balance(address) {
                account.balance = None
            }
            if account.nonce == prestate_cache.get_nonce(address) {
                account.nonce = None
            }
            if account.code == prestate_cache.get_code(address) {
                account.code = None
            }
            !account.is_empty()
        });

        updates
    }

    pub fn check_storage_writes(
        self,
        account_diffs: Vec<AccountDiff>,
        storage_writes: Vec<StorageWrite>,
        published_preimages: Vec<(alloy_primitives::FixedBytes<32>, Vec<u8>)>,
        prestate_cache: Cache,
        miner: B160,
        withdrawals: &[Withdrawal],
    ) -> Result<(), PostCheckError> {
        let diffs = self.collect_diffs(&prestate_cache, miner);
        let zksync_os_diffs = zksync_os_output_into_account_state(
            account_diffs,
            storage_writes,
            published_preimages,
            &prestate_cache,
        )?;

        // Reference => ZKsync OS check:
        for (address, account) in diffs.iter() {
            let zk_account = match zksync_os_diffs.get(address) {
                Some(v) => v,
                None => {
                    error!(
                        "ZKsync OS must have write for account {} {:?}",
                        hex::encode(address.to_be_bytes_vec()),
                        account
                    );
                    return Err(PostCheckError::Internal);
                }
            };
            if let Some(bal) = account.balance {
                // Balance might differ due to refunds and access list gas charging
                if Some(bal) != zk_account.balance {
                    error!(
                        "Balance for {} is {:?} but expected {:?}.\n  Difference: {:?}",
                        hex::encode(address.to_be_bytes_vec()),
                        zk_account.balance,
                        bal,
                        zk_account.balance.unwrap_or(U256::ZERO).abs_diff(bal),
                    );
                    return Err(PostCheckError::Internal);
                };
            }
            if let Some(nonce) = account.nonce {
                if nonce != zk_account.nonce.unwrap() {
                    error!(
                        "Nonce for address {} differed. ZKsync OS: {:?}, reference: {:?}",
                        hex::encode(address.to_be_bytes_vec()),
                        zk_account.nonce.unwrap(),
                        nonce
                    );
                    return Err(PostCheckError::Internal);
                }
            }
            if account.code.is_some() && account.code != zk_account.code {
                error!(
                    "Code for address {} differed. ZKsync OS: {}, reference: {}",
                    hex::encode(address.to_be_bytes_vec()),
                    hex::encode(zk_account.code.as_ref().unwrap_or_default()),
                    hex::encode(account.code.as_ref().unwrap_or_default())
                );
                return Err(PostCheckError::Internal);
            }
            if let Some(storage) = &account.storage {
                for (key, value) in storage {
                    let zksync_os_value = match zk_account.storage.as_ref().unwrap().get(key) {
                        Some(v) => v,
                        None => {
                            error!(
                                "Should have {:x} at address {}",
                                key,
                                hex::encode(address.to_be_bytes_vec())
                            );
                            return Err(PostCheckError::Internal);
                        }
                    };
                    if value != zksync_os_value {
                        error!(
                            "Value for slot 0x{:x} at address 0x{} differed. ZKsync OS: 0x{:x}, reference: 0x{:x}",
                            key,
                            hex::encode(address.to_be_bytes_vec()),
                            zksync_os_value.as_uint(),
                            value.as_uint()
                        );
                        return Err(PostCheckError::Internal);
                    }
                }

                for (k, v) in zk_account.storage.as_ref().unwrap().iter() {
                    // In the diff trace, slot clearing is not present in post,
                    // so we have to allow the case when v == 0.
                    if !(v.as_uint().is_zero() || storage.contains_key(k)) {
                        error!("Key {k:?} for {address:?} not present in reference");
                        return Err(PostCheckError::Internal);
                    }
                }
            }
        }

        // ZKsync OS => reference
        for (address, acc) in zksync_os_diffs.iter() {
            // Just check that it's part of the reference diffs,
            // all else should be checked already
            if address != &miner && !acc.is_empty() {
                match diffs.get(address) {
                    Some(_) => (),
                    None => {
                        // For some reason, selfdestruct is not correctly reported in the
                        // traces. We could use calltrace, but for now we just check that
                        // the ZKsync OS diff is consistent with selfdestruct.
                        if !zksync_os_diff_consistent_with_selfdestruct(
                            address,
                            acc,
                            &prestate_cache,
                            withdrawals,
                        ) {
                            error!(
                                "Reference must have write for account {} {:?}",
                                hex::encode(address.to_be_bytes_vec()),
                                acc
                            );
                            return Err(PostCheckError::Internal);
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

fn zksync_os_diff_consistent_with_selfdestruct(
    address: &B160,
    acc: &AccountState,
    prestate_cache: &Cache,
    _withdrawals: &[Withdrawal],
) -> bool {
    let diff_is_empty = acc.balance.is_none_or(|b| b.is_zero())
        && acc.nonce.is_none_or(|n| n == 0)
        && acc.code.as_ref().is_none_or(|c| c.is_empty())
        && acc.storage.as_ref().is_none_or(|s| s.is_empty());
    let pre = prestate_cache.0.get(address);
    let prestate_can_be_deployed = || {
        pre.is_none_or(|pre| {
            pre.storage.as_ref().is_none_or(|s| s.is_empty())
                && pre.code.as_ref().is_none_or(|c| c.is_empty())
                && pre.nonce.is_none_or(|n| n == 0)
        })
    };

    _withdrawals
        .iter()
        .any(|x| x.address.0 .0 == address.to_be_bytes())
        || (diff_is_empty && prestate_can_be_deployed())
}

fn zksync_os_output_into_account_state(
    account_diffs: Vec<AccountDiff>,
    storage_writes: Vec<StorageWrite>,
    published_preimages: Vec<(alloy_primitives::FixedBytes<32>, Vec<u8>)>,
    prestate_cache: &Cache,
) -> Result<HashMap<B160, AccountState>, PostCheckError> {
    use basic_system::system_implementation::flat_storage_model::AccountProperties;
    let mut updates: HashMap<B160, AccountState> = HashMap::new();
    let preimages: HashMap<[u8; 32], Vec<u8>> = HashMap::from_iter(
        published_preimages
            .into_iter()
            .map(|(key, value)| (key.0, value)),
    );
    for account_diff in account_diffs {
        let mut state = AccountState {
            balance: Some(account_diff.balance),
            nonce: Some(account_diff.nonce),
            code: None,
            storage: Some(BTreeMap::new()),
        };
        if let Some(bytecode) = preimages.get(&account_diff.bytecode_hash.0) {
            let owned: Vec<u8> = bytecode.to_owned();
            state.code = Some(owned.into());
        }
        let existing = updates.insert(
            B160::from_be_bytes(account_diff.address.into_array()),
            state,
        );
        assert!(existing.is_none());
    }
    for w in storage_writes {
        if rig::chain::is_account_properties_address(&B160::from_be_bytes(w.account.into_array())) {
            // populate account
            let address: [u8; 20] = w.account_key.as_slice()[12..].try_into().unwrap();
            let address = B160::from_be_bytes(address);
            if address != system_hooks::addresses_constants::BOOTLOADER_FORMAL_ADDRESS {
                let props = if w.value.is_zero() {
                    // TODO: Account deleted, we need to check this somehow
                    AccountProperties::default()
                } else {
                    let encoded = match preimages.get(w.value.as_slice()) {
                        Some(x) => x.clone(),
                        None => {
                            error!("Must contain preimage for account {address:#?}");
                            return Err(PostCheckError::Internal);
                        }
                    };
                    AccountProperties::decode(&encoded.try_into().unwrap())
                };
                assert!(!updates.contains_key(&address));
                let entry = updates.entry(address).or_default();
                entry.balance = Some(props.balance);
                entry.nonce = Some(props.nonce);
                if let Some(bytecode) = preimages.get(&props.bytecode_hash.as_u8_array()) {
                    let owned: Vec<u8> =
                        bytecode[..props.observable_bytecode_len as usize].to_owned();
                    entry.code = Some(owned.into());
                }
            }
        } else {
            // populate slot
            let address = w.account;
            let key = U256::from_be_bytes(w.account_key.0);
            let entry = updates
                .entry(B160::from_be_bytes(address.into_array()))
                .or_default();
            let value = B256::from_be_bytes(w.value.0);
            entry.storage.get_or_insert_default().insert(key, value);
        }
    }

    // Filter out empty diffs
    updates.retain(|address, account| {
        if let Some(storage) = account.storage.as_mut() {
            storage.retain(|key, new_val| match prestate_cache.get_slot(address, key) {
                None => *new_val != B256::ZERO,
                Some(initial) => *new_val != initial,
            })
        }
        if account.storage.as_ref().is_some_and(|s| s.is_empty()) {
            account.storage = None
        }
        if account.balance == prestate_cache.get_balance(address) {
            account.balance = None
        }
        if account.nonce == prestate_cache.get_nonce(address) {
            account.nonce = None
        }
        if account.code == prestate_cache.get_code(address) {
            account.code = None
        }
        !account.is_empty()
    });

    Ok(updates)
}

pub fn post_check(
    block_output: BlockOutput,
    receipts: Vec<TransactionReceipt>,
    diff_trace: DiffTrace,
    prestate_cache: Cache,
    miner: B160,
    withdrawals: &[Withdrawal],
) -> Result<(), PostCheckError> {
    assert_eq!(receipts.len(), block_output.tx_results.len());

    fn u256_to_usize(src: &U256) -> usize {
        zk_ee::utils::u256_to_u64_saturated(src) as usize
    }

    for (res, receipt) in block_output.tx_results.iter().zip(receipts.iter()) {
        // info!(
        //     "Checking transaction {} for consistency",
        //     receipt.transaction_index,
        // );
        let res = match res {
            Ok(res) => res,
            Err(e) => {
                error!(
                    "Transaction {} must be valid, failed with {:#?}",
                    receipt.transaction_hash, e
                );
                return Err(PostCheckError::InvalidTx {
                    id: TxId::Hash(receipt.transaction_hash.to_string()),
                });
            }
        };
        if receipt.status == Some(alloy::primitives::U256::ONE) {
            if !res.is_success() {
                error!(
                    "Transaction {} should have succeeded",
                    receipt.transaction_index
                );
                return Err(PostCheckError::InvalidTx {
                    id: TxId::Index(u256_to_usize(&receipt.transaction_index)),
                });
            };
        } else if receipt.status == Some(alloy::primitives::U256::ZERO) && res.is_success() {
            error!(
                "Transaction {} should have failed",
                receipt.transaction_index
            );
            return Err(PostCheckError::TxShouldHaveFailed {
                id: TxId::Index(u256_to_usize(&receipt.transaction_index)),
            });
        }
        // Logs check
        if res.logs.len() != receipt.logs.len() {
            error!(
                "Transaction {} has mismatch in number of logs",
                receipt.transaction_index
            );
            return Err(PostCheckError::IncorrectLogs {
                id: TxId::Index(u256_to_usize(&receipt.transaction_index)),
            });
        }
        for (l, r) in res.logs.iter().zip(receipt.logs.iter()) {
            let eq = r.is_equal_to_excluding_data(l);
            if !eq {
                error!("Not equal logs:\n {l:#?} \nand\n {r:?}");
                return Err(PostCheckError::IncorrectLogs {
                    id: TxId::Index(u256_to_usize(&receipt.transaction_index)),
                });
            }
            if r.data.to_vec() != l.data.data {
                error!(
                    "Data is not equal: we got {}, expected {}",
                    hex::encode(l.data.data.clone()),
                    hex::encode(r.data.clone())
                );
                return Err(PostCheckError::IncorrectLogs {
                    id: TxId::Index(u256_to_usize(&receipt.transaction_index)),
                });
            }
        }
        let gas_difference =
            zk_ee::utils::u256_to_u64_saturated(&receipt.gas_used).abs_diff(res.gas_used);
        // Check gas used
        if res.gas_used != zk_ee::utils::u256_to_u64_saturated(&receipt.gas_used) {
            error!(
                    "Transaction {} has a gas mismatch: ZKsync OS used {}, reference: {}\n  Difference:{}",
                    receipt.transaction_index, res.gas_used, receipt.gas_used,
                    gas_difference,
                );
            return Err(PostCheckError::GasMismatch {
                id: TxId::Index(u256_to_usize(&receipt.transaction_index)),
            });
        }
    }

    diff_trace.check_storage_writes(
        block_output.account_diffs,
        block_output.storage_writes,
        block_output.published_preimages,
        prestate_cache,
        miner,
        withdrawals,
    )?;

    info!("All good!");
    Ok(())
}

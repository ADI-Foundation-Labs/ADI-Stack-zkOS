use std::cmp::min;
use std::collections::HashMap;
use std::sync::Arc;

use crate::utils::*;
use alloy::consensus::transaction::*;
use alloy::consensus::*;
use alloy::eips::Encodable2718;
use alloy::network::TxSignerSync;
use alloy::primitives::*;
use alloy::rpc::types::TransactionRequest;
use alloy::signers::local::PrivateKeySigner;
use itertools::Itertools;
use std::collections::BTreeMap;
use zk_ee::system::tracer::NopTracer;
use zk_ee::utils::Bytes32;
use zksync_os_basic_bootloader::bootloader::block_flow::ethereum_block_flow::PectraForkHeader;
use zksync_os_basic_bootloader::bootloader::constants::MAX_BLOCK_GAS_LIMIT;
use zksync_os_basic_bootloader::bootloader::errors::InvalidTransaction;
use zksync_os_basic_system::system_implementation::ethereum_storage_model::caches::EMPTY_STRING_KECCAK_HASH;
use zksync_os_basic_system::system_implementation::ethereum_storage_model::EMPTY_ROOT_HASH;
use zksync_os_forward_system::run::errors::ForwardSubsystemError;
use zksync_os_forward_system::run::test_impl::{
    InMemoryPreimageSource, NoopTxCallback, TxListSource,
};
use zksync_os_forward_system::run::*;
use zksync_os_oracle_provider::*;

use crate::test::case::transaction::AccessListItem;
use crate::test::case::transaction::AuthorizationListItem;
use crate::test::case::transaction::Transaction;
use zksync_os_forward_system::run::result_keeper::ForwardRunningResultKeeper;
use zksync_os_basic_system::system_implementation::ethereum_storage_model::caches::account_properties::EthereumAccountProperties;
use crate::vm::zk_ee::ZKsyncOSEVMContext;
use crate::vm::zk_ee::ZKsyncOSExecutionResult;

///
/// Sign and encode alloy transaction request using provided `wallet`.
///
pub fn sign_and_encode_transaction_request(
    req: TransactionRequest,
    wallet: &PrivateKeySigner,
) -> Vec<u8> {
    let typed_tx = req.build_typed_tx().expect("Failed to build typed tx");
    match typed_tx {
        TypedTransaction::Legacy(tx) => sign_alloy_tx(tx, wallet, false),
        TypedTransaction::Eip1559(tx) => sign_alloy_tx(tx, wallet, true),
        TypedTransaction::Eip7702(tx) => sign_alloy_tx(tx, wallet, true),
        TypedTransaction::Eip2930(tx) => sign_alloy_tx(tx, wallet, true),
        TypedTransaction::Eip4844(tx) => sign_alloy_tx(tx, wallet, true),
    }
}

///
/// Sign and encode alloy transaction using provided `wallet`.
///
#[allow(deprecated)]
pub fn sign_alloy_tx<T: SignableTransaction<Signature>>(
    mut tx: T,
    wallet: &PrivateKeySigner,
    encode_eip_2718: bool,
) -> Vec<u8>
where
    T: RlpEcdsaEncodableTx,
{
    let signature = wallet.sign_transaction_sync(&mut tx).unwrap();

    let tx = tx.into_signed(signature);

    if encode_eip_2718 {
        tx.encoded_2718()
    } else {
        let mut result = vec![];
        tx.rlp_encode(&mut result);

        result
    }
}

///
/// The ZKsync OS + Ethereum STF interface.
///
#[derive(Clone)]
pub struct ZKsyncOSEthereumSTF {
    account_properties: HashMap<ruint::aliases::B160, EthereumAccountProperties>,
    cold_storage: HashMap<ruint::aliases::B160, HashMap<Bytes32, Bytes32>>,
    preimage_source: InMemoryPreimageSource,
}

impl ZKsyncOSEthereumSTF {
    pub fn new() -> Self {
        let preimage_source = InMemoryPreimageSource {
            inner: Default::default(),
        };
        Self {
            account_properties: HashMap::new(),
            cold_storage: HashMap::new(),
            preimage_source,
        }
    }

    pub fn clone(vm: Arc<Self>) -> Self {
        (*vm).clone()
    }

    pub fn execute_transaction(
        &mut self,
        transaction: &Transaction,
        system_context: ZKsyncOSEVMContext,
        bench: bool,
        test_id: String,
    ) -> anyhow::Result<ZKsyncOSExecutionResult, String> {
        let access_list = transaction.access_list.clone().map(|v| {
            alloy::eips::eip2930::AccessList(
                v.into_iter()
                    .map(
                        |AccessListItem {
                             address,
                             storage_keys,
                         }| {
                            let storage_keys = storage_keys
                                .into_iter()
                                .map(|k| {
                                    let buffer: [u8; 32] = k.to_be_bytes();
                                    alloy::primitives::FixedBytes::from_slice(&buffer)
                                })
                                .collect_vec();
                            alloy::eips::eip2930::AccessListItem {
                                address: alloy::primitives::Address::from_slice(address.as_ref()),
                                storage_keys,
                            }
                        },
                    )
                    .collect_vec(),
            )
        });

        use alloy::primitives::Signature;

        let authorization_list = transaction.authorization_list.clone().map(|v| {
            v.into_iter()
                .map(
                    |AuthorizationListItem {
                         nonce,
                         chain_id,
                         address,
                         v: _,
                         r,
                         s,
                         signer: _,
                         y_parity,
                     }| {
                        let mut r_buf = [0u8; 32];
                        r.to_big_endian(&mut r_buf);
                        let mut s_buf = [0u8; 32];
                        s.to_big_endian(&mut s_buf);
                        let y_parity = !y_parity.is_zero();

                        let signature = Signature::from_scalars_and_parity(
                            alloy::primitives::FixedBytes::from_slice(&r_buf),
                            alloy::primitives::FixedBytes::from_slice(&s_buf),
                            y_parity,
                        );
                        alloy::eips::eip7702::Authorization {
                            chain_id: chain_id.into(),
                            nonce: nonce.as_u64(),
                            address: alloy::primitives::Address::from_slice(address.as_ref()),
                        }
                        .into_signed(signature)
                    },
                )
                .collect_vec()
        });

        let request = alloy::rpc::types::TransactionRequest {
            chain_id: Some(system_context.chain_id),
            nonce: Some(transaction.nonce.try_into().expect("Nonce overflow")),
            max_fee_per_gas: Some(
                transaction
                    .max_fee_per_gas
                    .unwrap_or(system_context.gas_price)
                    .try_into()
                    .expect("Max fee per gas overflow"),
            ),
            max_priority_fee_per_gas: Some(
                transaction
                    .max_priority_fee_per_gas
                    .unwrap_or(system_context.gas_price)
                    .try_into()
                    .expect("Max priority fee per gas overflow"),
            ),
            gas: Some(
                transaction
                    .gas_limit
                    .try_into()
                    .expect("gas limit overflow"),
            ),
            to: Some(
                transaction
                    .to
                    .0
                    .map_or(alloy::primitives::TxKind::Create, |addr| {
                        alloy::primitives::TxKind::Call(alloy::primitives::Address::from_slice(
                            addr.as_ref(),
                        ))
                    }),
            ),
            value: Some(transaction.value.into()),
            input: transaction.data.clone().into(),
            access_list,
            authorization_list,
            ..Default::default()
        };

        let wallet = zksync_os_rig::alloy::signers::local::PrivateKeySigner::from_slice(
            transaction.secret_key.as_slice(),
        )
        .unwrap();
        let encoded_tx = sign_and_encode_transaction_request(request, &wallet);

        let tx_source = TxListSource {
            transactions: vec![encoded_tx].into(),
        };

        let block_gas_limit: u64 = system_context
            .block_gas_limit
            .try_into()
            .expect("Block gas limit overflowed u64");
        // Override block gas limit
        let gas_limit = min(block_gas_limit, MAX_BLOCK_GAS_LIMIT);

        // make header - Pectra header requires all optional fields to be Some
        let target_block_header = alloy::consensus::Header {
            parent_hash: Default::default(),
            ommers_hash: Default::default(),
            beneficiary: system_context.coinbase,
            state_root: Default::default(),
            transactions_root: Default::default(),
            receipts_root: Default::default(),
            logs_bloom: Default::default(),
            difficulty: U256::ONE,
            number: system_context.block_number as u64,
            gas_limit: gas_limit,
            gas_used: 0,
            timestamp: system_context.block_timestamp as u64,
            extra_data: Default::default(),
            mix_hash: Default::default(),
            nonce: Default::default(),
            base_fee_per_gas: Some(system_context.base_fee.try_into().unwrap()),
            withdrawals_root: Some(Default::default()),
            blob_gas_used: Some(Default::default()),
            excess_blob_gas: Some(Default::default()),
            parent_beacon_block_root: Some(Default::default()),
            requests_hash: Some(Default::default()),
        };

        let preimage_source = self.preimage_source.clone();

        // Output flamegraphs if on benchmarking mode
        if bench {
            unimplemented!();
        }

        use alloy::rlp::Encodable;
        let mut target_header_encoding = vec![];
        target_block_header.encode(&mut target_header_encoding);

        self.fix_storage();
        self.fix_account_properties();

        let target_header_reponsder = EthereumTargetBlockHeaderResponder {
            target_header: target_block_header,
            target_header_encoding,
        };
        let tx_data_responder = TxDataResponder {
            tx_source,
            next_tx: None,
        };
        let preimage_responder = GenericPreimageResponder { preimage_source };
        let initial_account_state_responder = InMemoryEthereumInitialAccountStateResponder {
            source: self.account_properties.clone(),
        };
        let initial_values_responder = InMemoryInitialStorageSlotValueResponder {
            values_map: self.cold_storage.clone(),
        };

        let mut blob_hashes = BTreeMap::new();
        // for blob in blobs.into_iter() {
        //     let versioned_hash = blob.to_kzg_versioned_hash();
        //     let point =
        //         crypto::bls12_381::G1Affine::deserialize_compressed(&blob.kzg_commitment.0[..])
        //             .unwrap();

        //     blob_hashes.insert(Bytes32::from_array(versioned_hash), point);
        // }

        let cl_responder = EthereumCLResponder {
            withdrawals_list: vec![],
            parent_headers_list: vec![],
            parent_headers_encodings_list: vec![],
            blob_hashes,
        };

        let mut oracle = ZkEENonDeterminismSource::default();
        oracle.add_external_processor(target_header_reponsder);
        oracle.add_external_processor(tx_data_responder);
        oracle.add_external_processor(preimage_responder);
        oracle.add_external_processor(initial_account_state_responder);
        oracle.add_external_processor(initial_values_responder);
        oracle.add_external_processor(cl_responder);
        oracle.add_external_processor(UARTPrintReponsder);

        use zksync_os_basic_bootloader::bootloader::config::BasicBootloaderForwardETHLikeConfig;
        use zksync_os_basic_bootloader::bootloader::BasicBootloader;
        use zksync_os_forward_system::run::result_keeper::ForwardRunningResultKeeper;
        use zksync_os_forward_system::system::system_types::ethereum::*;
        use zksync_os_oracle_provider::DummyMemorySource;

        let mut result_keeper = ForwardRunningResultKeeper::new(NoopTxCallback);
        let mut nop_tracer = NopTracer::default();
        let res = BasicBootloader::<
            EthereumStorageSystemTypes<ZkEENonDeterminismSource<DummyMemorySource>>,
        >::run::<BasicBootloaderForwardETHLikeConfig>(
            oracle, &mut result_keeper, &mut nop_tracer
        );

        match res {
            Ok(_) => self.apply_batch_execution_result(Ok(result_keeper)),
            Err(e) => {
                panic!("System error {:?}", e);
                // self.apply_batch_execution_result(Err(e))
            }
        }
    }

    fn apply_batch_execution_result(
        &mut self,
        batch_execution_result: Result<
            ForwardRunningResultKeeper<NoopTxCallback, PectraForkHeader>,
            ForwardSubsystemError,
        >,
    ) -> anyhow::Result<ZKsyncOSExecutionResult, String> {
        match batch_execution_result {
            Ok(result) => {
                for storage_write in result.storage_writes.iter() {
                    self.set_storage_slot(
                        Address::from(storage_write.0.to_be_bytes::<20>()),
                        ruint::aliases::U256::from_be_bytes(storage_write.1.as_u8_array()),
                        FixedBytes::<32>::from(storage_write.2.as_u8_array()),
                    );
                }

                for (hash, preimage, _) in result.new_preimages.iter() {
                    self.preimage_source.inner.insert(*hash, preimage.clone());
                }

                for (addr, props) in result.account_diffs.iter() {
                    let props = EthereumAccountProperties {
                        nonce: props.0,
                        balance: props.1,
                        storage_root: Bytes32::ZERO,
                        bytecode_hash: props.2,
                        computed_is_unset: false,
                    };
                    let addr = Address::from(addr.to_be_bytes::<20>());
                    self.set_account_properties(addr, props);
                }

                self.fix_storage();
                self.fix_account_properties();

                use zksync_os_forward_system::run::output::map_tx_results;
                let tx_results = map_tx_results(&result);

                let tx_result = tx_results.get(0).expect("Do not have tx output").clone();

                Self::get_transaction_execution_result(tx_result)
            }
            Err(err) => Err(format!("{err:?}")),
        }
    }

    fn get_transaction_execution_result(
        tx_result: Result<TxOutput, InvalidTransaction>,
    ) -> anyhow::Result<ZKsyncOSExecutionResult, String> {
        match tx_result {
            Ok(tx_output) => {
                let mut execution_result = ZKsyncOSExecutionResult::default();

                execution_result.gas = U256::from(tx_output.gas_used);
                // TODO events

                match &tx_output.execution_result {
                    zksync_os_forward_system::run::ExecutionResult::Success(execution_output) => {
                        match execution_output {
                            zksync_os_forward_system::run::ExecutionOutput::Call(data) => {
                                execution_result.return_data = data.clone();
                            }
                            zksync_os_forward_system::run::ExecutionOutput::Create(
                                data,
                                address,
                            ) => {
                                let bytes = address.to_be_bytes();
                                execution_result.return_data = data.clone();
                                execution_result.address_deployed = Some(Address::from(bytes));
                            }
                        }
                    }
                    zksync_os_forward_system::run::ExecutionResult::Revert(vec) => {
                        execution_result.exception = true;
                        execution_result.return_data = vec.clone();
                    }
                }
                Ok(execution_result)
            }
            Err(tx_err) => Err(format!("{tx_err:?}")),
        }
    }

    fn get_account_properties(&mut self, address: Address) -> EthereumAccountProperties {
        match self.account_properties.get(&address_to_b160(address)) {
            None => EthereumAccountProperties::default(),
            Some(account_hash) => account_hash.clone(),
        }
    }

    fn set_account_properties(&mut self, address: Address, properties: EthereumAccountProperties) {
        let address = address_to_b160(address);
        self.account_properties.insert(address, properties);
    }

    ///
    /// Returns the balance of the specified address.
    ///
    pub fn get_balance(&mut self, address: Address) -> U256 {
        let properties = self.get_account_properties(address);
        properties.balance
    }

    ///
    /// Changes the balance of the specified address.
    ///
    pub fn set_balance(&mut self, address: Address, value: U256) {
        let mut properties = self.get_account_properties(address);
        properties.balance = value;
        self.set_account_properties(address, properties)
    }

    ///
    /// Returns the nonce of the specified address.
    ///
    pub fn get_nonce(&mut self, address: Address) -> U256 {
        let properties = self.get_account_properties(address);
        U256::from(properties.nonce)
    }

    ///
    /// Changes the nonce of the specified address.
    ///
    pub fn set_nonce(&mut self, address: Address, value: U256) {
        let mut properties = self.get_account_properties(address);
        properties.nonce = value.try_into().expect("nonce overflow");
        self.set_account_properties(address, properties)
    }

    pub fn get_storage_slot(&mut self, address: Address, key: U256) -> Option<B256> {
        let address = address_to_b160(address);
        let key = u256_to_bytes32(key);

        let storage = self.cold_storage.get(&address)?;
        let value = storage.get(&key)?;
        Some(bytes32_to_b256(*value))
    }

    pub fn set_storage_slot(&mut self, address: Address, key: U256, value: B256) {
        let address = address_to_b160(address);
        let key = u256_to_bytes32(key);

        let value = b256_to_bytes32(value);
        self.cold_storage
            .entry(address)
            .or_default()
            .insert(key, value);
    }

    pub fn evm_bytecode_into_account_properties(
        &mut self,
        address: Address,
        bytecode: &[u8],
    ) -> (EthereumAccountProperties, Vec<u8>) {
        use zksync_os_crypto::MiniDigest;
        let mut result = self.get_account_properties(address);
        let observable_bytecode_hash =
            Bytes32::from_array(zksync_os_crypto::sha3::Keccak256::digest(bytecode));
        result.bytecode_hash = observable_bytecode_hash;

        self.set_account_properties(address, result);
        self.preimage_source
            .inner
            .insert(observable_bytecode_hash, bytecode.to_vec());

        (result, bytecode.to_vec())
    }

    pub fn set_predeployed_evm_contract(&mut self, address: Address, bytecode: Bytes, nonce: U256) {
        let (mut account_data, bytecode) =
            self.evm_bytecode_into_account_properties(address, &bytecode);
        account_data.nonce = nonce.try_into().expect("nonce overflow");
        self.preimage_source
            .inner
            .insert(account_data.bytecode_hash, bytecode.to_vec());
    }

    pub fn get_code(&mut self, address: Address) -> Option<Vec<u8>> {
        let properties = self.get_account_properties(address);
        let bytecode_hash = properties.bytecode_hash;

        if bytecode_hash == Bytes32::zero() || bytecode_hash == EMPTY_STRING_KECCAK_HASH {
            None
        } else {
            Some(
                self.preimage_source
                    .inner
                    .get(&bytecode_hash)
                    .unwrap()
                    .to_vec(),
            )
        }
    }

    fn fix_storage(&mut self) {
        for (_, storage) in self.cold_storage.iter_mut() {
            storage.retain(|_, v| v.is_zero() == false);
        }
    }

    fn fix_account_properties(&mut self) {
        for (addr, props) in self.account_properties.iter_mut() {
            let is_empty_storage = if let Some(storage) = self.cold_storage.get(addr) {
                storage.is_empty()
            } else {
                true
            };
            if is_empty_storage {
                props.storage_root = EMPTY_ROOT_HASH;
            }
            fix_account_properties(props, is_empty_storage);
        }
    }
}

pub fn b256_to_bytes32(input: B256) -> Bytes32 {
    Bytes32::from_array(input.0)
}

pub fn u256_to_bytes32(input: U256) -> Bytes32 {
    Bytes32::from_array(input.to_be_bytes())
}

pub fn bytes32_to_b256(input: Bytes32) -> B256 {
    B256::from_slice(&input.as_u8_array())
}

pub fn fix_account_properties(props: &mut EthereumAccountProperties, storage_is_empty: bool) {
    if props.balance.is_zero()
        && props.nonce == 0
        && props.bytecode_hash == EMPTY_STRING_KECCAK_HASH
        && storage_is_empty
    {
        props.computed_is_unset = true;
    } else {
        props.computed_is_unset = false;
    }
}

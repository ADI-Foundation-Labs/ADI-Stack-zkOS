use crate::bootloader::transaction_flow::*;
use alloc::vec::Vec;
use basic_system::system_implementation::flat_storage_model::FlatStorageCommitment;
use basic_system::system_implementation::flat_storage_model::TREE_HEIGHT;
use basic_system::system_implementation::system::{
    DefaultHeaderStructurePostWork, SystemPostWork, TypedFinishIO,
};
use constants::{MAX_TX_LEN_WORDS, TX_OFFSET_WORDS};
use errors::BootloaderSubsystemError;
use result_keeper::ResultKeeperExt;
use ruint::aliases::*;
use zk_ee::execution_environment_type::ExecutionEnvironmentType;
use zk_ee::system::tracer::Tracer;
use zk_ee::system::{EthereumLikeTypes, System, SystemTypes};

pub mod run_single_interaction;
pub mod runner;
pub mod supported_ees;

pub mod ethereum_eoa_flow;
mod gas_helpers;
mod paymaster_helper;
mod process_transaction;
pub mod transaction;
pub mod transaction_flow;

pub mod block_header;
pub mod config;
pub mod constants;
pub mod errors;
pub mod result_keeper;
mod rlp;

use alloc::boxed::Box;
use core::alloc::Allocator;
use core::fmt::Write;
use core::mem::MaybeUninit;
use crypto::MiniDigest;
use zk_ee::{internal_error, oracle::*};

use crate::bootloader::block_header::BlockHeader;
use crate::bootloader::config::BasicBootloaderExecutionConfig;
use crate::bootloader::constants::TX_OFFSET;
use crate::bootloader::errors::TxError;
use crate::bootloader::result_keeper::*;
use crate::bootloader::runner::RunnerMemoryBuffers;
use system_hooks::HooksStorage;
use zk_ee::system::*;
use zk_ee::utils::*;

pub(crate) const EVM_EE_BYTE: u8 = ExecutionEnvironmentType::EVM_EE_BYTE;
pub const DEBUG_OUTPUT: bool = false;

pub struct BasicBootloader<S: EthereumLikeTypes> {
    _marker: core::marker::PhantomData<S>,
}

struct TxDataBuffer<A: Allocator> {
    buffer: Vec<u32, A>,
}

impl<A: Allocator> TxDataBuffer<A> {
    fn new(allocator: A) -> Self {
        let mut buffer: Vec<u32, A> =
            Vec::with_capacity_in(TX_OFFSET_WORDS + MAX_TX_LEN_WORDS, allocator);
        buffer.resize(TX_OFFSET_WORDS, 0u32);

        Self { buffer }
    }

    #[allow(clippy::wrong_self_convention)]
    fn into_writable<'a>(&'a mut self) -> TxDataBufferWriter<'a> {
        self.buffer.resize(TX_OFFSET_WORDS, 0u32);
        let capacity = self.buffer.spare_capacity_mut();

        TxDataBufferWriter {
            capacity,
            offset: 0,
        }
    }

    fn as_tx_buffer<'a>(&'a mut self, next_tx_data_len_bytes: usize) -> &'a mut [u8] {
        let word_len = TX_OFFSET_WORDS
            + next_tx_data_len_bytes.next_multiple_of(core::mem::size_of::<u32>())
                / core::mem::size_of::<u32>();
        assert!(self.buffer.capacity() >= word_len);
        unsafe {
            self.buffer.set_len(word_len);
            core::slice::from_raw_parts_mut(
                self.buffer.as_mut_ptr().cast(),
                TX_OFFSET + next_tx_data_len_bytes,
            )
        }
    }
}

struct TxDataBufferWriter<'a> {
    capacity: &'a mut [MaybeUninit<u32>],
    offset: usize,
}

impl<'a> UsizeWriteable for TxDataBufferWriter<'a> {
    unsafe fn write_usize(&mut self, value: usize) {
        #[cfg(target_pointer_width = "32")]
        {
            if self.offset >= self.capacity.len() {
                panic!();
            }
            self.capacity[self.offset].write(value as u32);
            self.offset += 1;
        }

        #[cfg(target_pointer_width = "64")]
        {
            if self.offset + 1 >= self.capacity.len() {
                panic!();
            }
            self.capacity[self.offset].write(value as u32);
            self.capacity[self.offset + 1].write((value >> 32) as u32);
            self.offset += 2;
        }

        #[cfg(not(any(target_pointer_width = "32", target_pointer_width = "64")))]
        {
            compile_error!("unsupported arch")
        }
    }
}

impl<'a> SafeUsizeWritable for TxDataBufferWriter<'a> {
    fn try_write(&mut self, value: usize) -> Result<(), ()> {
        #[cfg(target_pointer_width = "32")]
        {
            if self.offset >= self.capacity.len() {
                return Err(());
            }
            self.capacity[self.offset].write(value as u32);
            self.offset += 1;

            Ok(())
        }

        #[cfg(target_pointer_width = "64")]
        {
            if self.offset + 1 >= self.capacity.len() {
                return Err(());
            }
            self.capacity[self.offset].write(value as u32);
            self.capacity[self.offset + 1].write((value >> 32) as u32);
            self.offset += 2;

            Ok(())
        }
    }

    fn len(&self) -> usize {
        if core::mem::size_of::<usize>() == core::mem::size_of::<u32>() {
            self.capacity.len()
        } else if core::mem::size_of::<usize>() == core::mem::size_of::<u64>() {
            self.capacity.len() / 2
        } else {
            unreachable!()
        }
    }
}

pub trait TxHashesCollector {
    fn add_tx_hash(&mut self, tx_hash: &Bytes32);
    fn finish(self) -> Bytes32;
}

pub struct RollingKeccakHash {
    inner: Bytes32,
    hasher: crypto::sha3::Keccak256,
}

pub struct AccumulatingBlake2sHash {
    hasher: crypto::blake2s::Blake2s256,
}

impl TxHashesCollector for () {
    fn add_tx_hash(&mut self, _tx_hash: &Bytes32) {}
    fn finish(self) -> Bytes32 {
        Bytes32::ZERO
    }
}

impl EnforcedTxCollector for () {
    fn add_enforced_tx_hash(&mut self, _tx_hash: &Bytes32) {}
    fn finish(self) -> Bytes32 {
        Bytes32::ZERO
    }
}

impl TxHashesCollector for RollingKeccakHash {
    fn add_tx_hash(&mut self, tx_hash: &Bytes32) {
        if self.inner.is_zero() {
            self.inner = *tx_hash;
        } else {
            self.inner = Bytes32::from_array({
                self.hasher.update(self.inner.as_u8_array_ref());
                self.hasher.update(tx_hash.as_u8_array_ref());
                self.hasher.finalize_reset()
            });
        }
    }

    fn finish(self) -> Bytes32 {
        self.inner
    }
}

pub trait UpgradeTxCollector {
    fn add_upgrade_tx_hash(&mut self, tx_hash: &Bytes32);
    fn finish(self) -> Bytes32;
}

pub struct UpgradeTx {
    inner: Bytes32,
}

impl UpgradeTxCollector for UpgradeTx {
    fn add_upgrade_tx_hash(&mut self, tx_hash: &Bytes32) {
        if self.inner.is_zero() == false {
            panic!("duplicate upgrade tx");
        }
        self.inner = *tx_hash;
    }

    fn finish(self) -> Bytes32 {
        self.inner
    }
}

pub trait EnforcedTxCollector {
    fn add_enforced_tx_hash(&mut self, tx_hash: &Bytes32);
    fn finish(self) -> Bytes32;
}

impl EnforcedTxCollector for AccumulatingBlake2sHash {
    fn add_enforced_tx_hash(&mut self, tx_hash: &Bytes32) {
        self.hasher.update(tx_hash.as_u8_array_ref());
    }

    fn finish(self) -> Bytes32 {
        Bytes32::from_array(self.hasher.finalize())
    }
}

pub trait BlockGasCounter {
    fn count_gas(&mut self, gas_used: u64);
}

impl BlockGasCounter for u64 {
    fn count_gas(&mut self, gas_used: u64) {
        *self += gas_used;
    }
}

impl<S: EthereumLikeTypes> BasicBootloader<S> {
    /// Runs the transactions that it loads from the oracle.
    /// This code runs both in sequencer (then it uses ForwardOracle - that stores data in local variables)
    /// and in prover (where oracle uses CRS registers to communicate).
    pub fn run_tx_loop<Config: BasicBootloaderExecutionConfig>(
        oracle: <S::IO as IOSubsystemExt>::IOOracle,
        result_keeper: &mut impl ResultKeeperExt,
        tx_hashes_collector: &mut impl TxHashesCollector,
        upgrade_tx_collector: &mut impl UpgradeTxCollector,
        enforced_tx_collector: &mut impl EnforcedTxCollector,
        gas_counter: &mut impl BlockGasCounter,
        tracer: &mut impl Tracer<S>,
    ) -> Result<System<S>, BootloaderSubsystemError>
    where
        S::IO: IOSubsystemExt,
    {
        cycle_marker::start!("run_tx_loop");
        // we will model initial calldata buffer as just another "heap"
        let mut system: System<S> =
            System::init_from_oracle(oracle).expect("system must be able to initialize itself");

        let mut initial_calldata_buffer = TxDataBuffer::new(system.get_allocator());

        pub const MAX_HEAP_BUFFER_SIZE: usize = 1 << 27; // 128 MB
        pub const MAX_RETURN_BUFFER_SIZE: usize = 1 << 27; // 128 MB

        let mut heaps = Box::new_uninit_slice_in(MAX_HEAP_BUFFER_SIZE, system.get_allocator());
        let mut return_data =
            Box::new_uninit_slice_in(MAX_RETURN_BUFFER_SIZE, system.get_allocator());

        let mut memories = RunnerMemoryBuffers {
            heaps: &mut heaps,
            return_data: &mut return_data,
        };

        let mut system_functions = HooksStorage::new_in(system.get_allocator());

        system_functions.add_precompiles();

        #[cfg(not(feature = "disable_system_contracts"))]
        {
            system_functions.add_l1_messenger();
            system_functions.add_l2_base_token();
            system_functions.add_contract_deployer();
        }

        let mut tx_counter = 0;

        // now we can run every transaction
        while let Some(next_tx_data_len_bytes) = {
            let mut writable = initial_calldata_buffer.into_writable();
            system
                .try_begin_next_tx(&mut writable)
                .expect("TX start call must always succeed")
        } {
            // warm up the coinbase formally
            {
                let mut inf_resources = S::Resources::FORMAL_INFINITE;
                system
                    .io
                    .read_account_properties(
                        ExecutionEnvironmentType::NoEE,
                        &mut inf_resources,
                        &system.get_coinbase(),
                        AccountDataRequest::empty(),
                    )
                    .expect("must heat coinbase");
            }

            let mut logger: <S as SystemTypes>::Logger = system.get_logger();
            let _ = logger.write_fmt(format_args!("====================================\n"));
            let _ = logger.write_fmt(format_args!(
                "TX execution begins for transaction {}\n",
                tx_counter
            ));

            let initial_calldata_buffer =
                initial_calldata_buffer.as_tx_buffer(next_tx_data_len_bytes);

            tracer.begin_tx(initial_calldata_buffer);

            // We will give the full buffer here, and internally we will use parts of it to give forward to EEs
            cycle_marker::start!("process_transaction");

            let tx_result = Self::process_transaction::<Config>(
                initial_calldata_buffer,
                &mut system,
                &mut system_functions,
                memories.reborrow(),
                tx_counter == 0,
                tracer,
            );

            cycle_marker::end!("process_transaction");

            tracer.finish_tx();

            match tx_result {
                Err(TxError::Internal(err)) => {
                    let _ = system.get_logger().write_fmt(format_args!(
                        "Tx execution result: Internal error = {err:?}\n",
                    ));
                    return Err(err);
                }
                Err(TxError::Validation(err)) => {
                    let _ = system.get_logger().write_fmt(format_args!(
                        "Tx execution result: Validation error = {err:?}\n",
                    ));
                    result_keeper.tx_processed(Err(err));
                }
                Ok(tx_processing_result) => {
                    // TODO: debug implementation for ruint types uses global alloc, which panics in ZKsync OS
                    #[cfg(not(target_arch = "riscv32"))]
                    let _ = system.get_logger().write_fmt(format_args!(
                        "Tx execution result = {:?}\n",
                        &tx_processing_result,
                    ));
                    let (status, output, contract_address) = match tx_processing_result.result {
                        ExecutionResult::Success { output } => match output {
                            ExecutionOutput::Call(output) => (true, output, None),
                            ExecutionOutput::Create(output, contract_address) => {
                                (true, output, Some(contract_address))
                            }
                        },
                        ExecutionResult::Revert { output } => (false, output, None),
                    };
                    gas_counter.count_gas(tx_processing_result.gas_used);
                    result_keeper.tx_processed(Ok(TxProcessingOutput {
                        status,
                        output: &output,
                        contract_address,
                        gas_used: tx_processing_result.gas_used,
                        gas_refunded: tx_processing_result.gas_refunded,
                        computational_native_used: tx_processing_result.computational_native_used,
                        pubdata_used: tx_processing_result.pubdata_used,
                    }));

                    tx_hashes_collector.add_tx_hash(&tx_processing_result.tx_hash);

                    if tx_processing_result.is_l1_tx {
                        enforced_tx_collector.add_enforced_tx_hash(&tx_processing_result.tx_hash);
                    }

                    if tx_processing_result.is_upgrade_tx {
                        upgrade_tx_collector.add_upgrade_tx_hash(&tx_processing_result.tx_hash);
                    }
                }
            }

            let tx_stats = system.flush_tx();
            let _ = system
                .get_logger()
                .write_fmt(format_args!("Tx stats = {tx_stats:?}\n"));

            let mut logger = system.get_logger();
            let _ = logger.write_fmt(format_args!(
                "TX execution ends for transaction {}\n",
                tx_counter
            ));
            let _ = logger.write_fmt(format_args!("====================================\n"));

            tx_counter += 1;
        }

        let _ = system
            .get_logger()
            .write_fmt(format_args!("Bootloader completed\n"));

        let mut logger = system.get_logger();
        let _ = logger.write_fmt(format_args!(
            "Bootloader execution is complete, will proceed with applying changes\n"
        ));

        cycle_marker::end!("run_tx_loop");

        Ok(system)
    }

    /// Runs the transactions that it loads from the oracle.
    /// This code runs both in sequencer (then it uses ForwardOracle - that stores data in local variables)
    /// and in prover (where oracle uses CRS registers to communicate).
    pub fn run_prepared<Config: BasicBootloaderExecutionConfig, const PROOF_ENV: bool>(
        oracle: <S::IO as IOSubsystemExt>::IOOracle,
        result_keeper: &mut impl ResultKeeperExt,
        tracer: &mut impl Tracer<S>,
    ) -> Result<
        <DefaultHeaderStructurePostWork<PROOF_ENV> as SystemPostWork<S>>::FinalData,
        BootloaderSubsystemError,
    >
    where
        S::IO: IOSubsystemExt
            + TypedFinishIO<
                FinalData = <S::IO as IOSubsystemExt>::IOOracle,
                IOStateCommittment = FlatStorageCommitment<TREE_HEIGHT>,
            >,
        DefaultHeaderStructurePostWork<PROOF_ENV>: SystemPostWork<S>, // As we can not tell that boolean is only true/false ...
    {
        cycle_marker::start!("run_prepared");

        // Here we use "default" implementation for our STF

        let mut tx_hashes_collector = RollingKeccakHash {
            inner: Bytes32::ZERO,
            hasher: crypto::sha3::Keccak256::new(),
        };

        let mut upgrade_tx_monitor = UpgradeTx {
            inner: Bytes32::ZERO,
        };

        let mut l1_to_l2_tx_hasher = AccumulatingBlake2sHash {
            hasher: crypto::blake2s::Blake2s256::new(),
        };

        let mut block_gas_used = 0u64;

        let system = Self::run_tx_loop::<Config>(
            oracle,
            result_keeper,
            &mut tx_hashes_collector,
            &mut upgrade_tx_monitor,
            &mut l1_to_l2_tx_hasher,
            &mut block_gas_used,
            tracer,
        )?;

        let tx_rolling_hash = tx_hashes_collector.finish();
        let l1_to_l2_tx_hash = l1_to_l2_tx_hasher.finish();
        let upgrade_tx_hash = upgrade_tx_monitor.finish();

        let block_number = system.get_block_number();
        let previous_block_hash = system.get_blockhash(block_number);
        let beneficiary = system.get_coinbase();
        // TODO: Gas limit should be constant
        let gas_limit = system.get_gas_limit();
        // TODO: gas used shouldn't be zero
        let timestamp = system.get_timestamp();
        let consensus_random = Bytes32::from_u256_be(&system.get_mix_hash());
        let base_fee_per_gas = system.get_eip1559_basefee();
        // TODO: add gas_per_pubdata and native price
        let base_fee_per_gas = base_fee_per_gas
            .try_into()
            .map_err(|_| internal_error!("base_fee_per_gas exceeds max u64"))?;
        let block_header = BlockHeader::new(
            Bytes32::from(previous_block_hash.to_be_bytes::<32>()),
            beneficiary,
            tx_rolling_hash,
            block_number,
            gas_limit,
            block_gas_used,
            timestamp,
            consensus_random,
            base_fee_per_gas,
        );
        let block_hash = Bytes32::from(block_header.hash());
        result_keeper.block_sealed(block_header);

        #[cfg(not(target_arch = "riscv32"))]
        cycle_marker::log_marker(
            format!(
                "Spent ergs for [run_prepared]: {}",
                result_keeper.get_gas_used() * evm_interpreter::ERGS_PER_GAS
            )
            .as_str(),
        );

        let mut logger = system.get_logger();
        let _ = logger.write_fmt(format_args!("Basic header information was created\n"));

        let finisher = DefaultHeaderStructurePostWork::<PROOF_ENV> {
            current_block_hash: block_hash,
            upgrade_tx_hash,
            l1_to_l2_txs_hash: l1_to_l2_tx_hash,
        };

        let result = finisher.finish(system, result_keeper, &mut logger);

        cycle_marker::end!("run_prepared");

        #[allow(clippy::let_and_return)]
        Ok(result)
    }

    pub fn run_for_state_root_only<Config: BasicBootloaderExecutionConfig, const PROOF_ENV: bool>(
        oracle: <S::IO as IOSubsystemExt>::IOOracle,
        result_keeper: &mut impl ResultKeeperExt,
        tracer: &mut impl Tracer<S>,
    ) -> Result<<S::IO as TypedFinishIO>::IOStateCommittment, BootloaderSubsystemError>
    where
        S::IO: IOSubsystemExt + TypedFinishIO,
    {
        cycle_marker::start!("run_for_state_root_only");

        // Mini-STF that only produces final state root

        let mut tx_hashes_collector = ();

        let mut upgrade_tx_monitor = UpgradeTx {
            inner: Bytes32::ZERO,
        };

        let mut l1_to_l2_tx_hasher = ();

        let mut block_gas_used = 0u64;

        let system = Self::run_tx_loop::<Config>(
            oracle,
            result_keeper,
            &mut tx_hashes_collector,
            &mut upgrade_tx_monitor,
            &mut l1_to_l2_tx_hasher,
            &mut block_gas_used,
            tracer,
        )?;

        let _ = TxHashesCollector::finish(tx_hashes_collector);
        let _ = EnforcedTxCollector::finish(l1_to_l2_tx_hasher);
        let _ = upgrade_tx_monitor.finish();

        let mut logger = system.get_logger();
        let _ = logger.write_fmt(format_args!(
            "Bootloader completed, will proceed with state update\n"
        ));

        {
            let block_number = system.get_block_number();
            let previous_block_hash = system.get_blockhash(block_number);
            let beneficiary = system.get_coinbase();
            // TODO: Gas limit should be constant
            let gas_limit = system.get_gas_limit();
            // TODO: gas used shouldn't be zero
            let timestamp = system.get_timestamp();
            let consensus_random = Bytes32::from_u256_be(&system.get_mix_hash());
            let base_fee_per_gas = system.get_eip1559_basefee();
            // TODO: add gas_per_pubdata and native price
            let base_fee_per_gas = base_fee_per_gas
                .try_into()
                .map_err(|_| internal_error!("base_fee_per_gas exceeds max u64"))?;
            let block_header = BlockHeader::new(
                Bytes32::from(previous_block_hash.to_be_bytes::<32>()),
                beneficiary,
                Bytes32::ZERO,
                block_number,
                gas_limit,
                block_gas_used,
                timestamp,
                consensus_random,
                base_fee_per_gas,
            );
            result_keeper.block_sealed(block_header);
        }

        let System { mut io, .. } = system;

        let initial_state_commitment = {
            use zk_ee::system_io_oracle::IOOracle;
            use zk_ee::system_io_oracle::INITIAL_STATE_COMMITTMENT_QUERY_ID;

            io.oracle()
                .query_with_empty_input::<<S::IO as TypedFinishIO>::IOStateCommittment>(
                    INITIAL_STATE_COMMITTMENT_QUERY_ID,
                )
                .unwrap()
        };
        let _ = logger.write_fmt(format_args!(
            "Initial state commitment is {:?}\n",
            &initial_state_commitment
        ));

        let mut state_commitment = initial_state_commitment.clone();
        let _ = io.finish(
            Some(&mut state_commitment),
            &mut NopHasher,
            &mut NopHasher,
            result_keeper,
            &mut logger,
        );

        let _ = logger.write_fmt(format_args!("State update from IO completed\n"));

        cycle_marker::end!("run_for_state_root_only");

        Ok(state_commitment)
    }
}

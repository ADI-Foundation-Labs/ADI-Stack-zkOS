use crate::bootloader::block_flow::MetadataInitOp;
use crate::bootloader::block_flow::PostSystemInitOp;
use crate::bootloader::block_flow::PostTxLoopOp;
use crate::bootloader::block_flow::PreTxLoopOp;
use crate::bootloader::block_flow::TxLoopOp;
use crate::bootloader::stf::EthereumLikeBasicSTF;
use crate::bootloader::transaction_flow::*;
use alloc::vec::Vec;
use constants::{MAX_TX_LEN_WORDS, TX_OFFSET_WORDS};
use errors::BootloaderSubsystemError;
use result_keeper::ResultKeeperExt;
use ruint::aliases::*;
use zk_ee::execution_environment_type::ExecutionEnvironmentType;
use zk_ee::system::tracer::Tracer;
use zk_ee::system::{EthereumLikeTypes, System, SystemTypes};

pub mod block_flow;
pub mod run_single_interaction;
pub mod runner;
pub mod supported_ees;

mod gas_helpers;
mod paymaster_helper;
mod process_l1_transaction;
mod process_transaction;
pub mod transaction;
pub mod transaction_flow;

pub mod block_header;
pub mod config;
pub mod constants;
pub mod errors;
pub mod result_keeper;
mod rlp;
pub mod stf;

use alloc::boxed::Box;
use core::alloc::Allocator;
use core::fmt::Write;
use core::mem::MaybeUninit;
use crypto::MiniDigest;
use zk_ee::oracle::*;

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

pub struct TxDataBuffer<A: Allocator> {
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

impl<S: EthereumLikeBasicSTF> BasicBootloader<S>
where
    <S as SystemTypes>::IO: IOSubsystemExt + IOTeardown<S::IOTypes>,
{
    /// Run STF to completion assuming that STF has simple structure
    pub fn run<Config: BasicBootloaderExecutionConfig>(
        mut oracle: <S::IO as IOSubsystemExt>::IOOracle,
        result_keeper: &mut impl ResultKeeperExt<S::IOTypes>,
        tracer: &mut impl Tracer<S>,
    ) -> Result<<S::PostTxLoopOp as PostTxLoopOp<S>>::PostTxLoopOpResult, BootloaderSubsystemError>
    where
        <S as SystemTypes>::IO: IOSubsystemExt + IOTeardown<S::IOTypes>,
    {
        // initialize the system
        cycle_marker::start!("system_init");
        // we will model initial calldata buffer as just another "heap"
        let metadata = <S::MetadataOp as MetadataInitOp<S>>::metadata_op::<Config>(
            &mut oracle,
            S::Allocator::default(),
        )?;

        let mut system: System<S> = System::init_from_metadata_and_oracle(metadata, oracle)?;
        let mut system_functions = HooksStorage::new_in(system.get_allocator());

        <S::PostSystemInitOp as PostSystemInitOp<S>>::post_init_op::<Config>(
            &mut system,
            &mut system_functions,
        )?;

        pub const MAX_HEAP_BUFFER_SIZE: usize = 1 << 27; // 128 MB
        pub const MAX_RETURN_BUFFER_SIZE: usize = 1 << 27; // 128 MB

        let mut heaps = Box::new_uninit_slice_in(MAX_HEAP_BUFFER_SIZE, system.get_allocator());
        let mut return_data =
            Box::new_uninit_slice_in(MAX_RETURN_BUFFER_SIZE, system.get_allocator());

        let memories = RunnerMemoryBuffers {
            heaps: &mut heaps,
            return_data: &mut return_data,
        };

        cycle_marker::end!("system_init");

        // Pre-op
        let mut block_data_keeper =
            <S::PreTxLoopOp as PreTxLoopOp<S>>::pre_op(&mut system, result_keeper);

        // TX loop
        <S::TxLoopOp as TxLoopOp<S>>::loop_op::<Config>(
            &mut system,
            &mut system_functions,
            memories,
            &mut block_data_keeper,
            result_keeper,
            tracer,
        )?;

        // whatever the non-persistent data was there, it's now gone

        // Post-op
        let result =
            <S::PostTxLoopOp as PostTxLoopOp<S>>::post_op(system, block_data_keeper, result_keeper);

        Ok(result)
    }
}

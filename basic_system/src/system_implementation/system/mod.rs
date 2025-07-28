//! Implementation of the system interface.
use core::alloc::Allocator;

use errors::SystemError;
use evm_interpreter::{
    gas_constants::{
        COLD_SLOAD_COST, SSTORE_RESET_EXTRA, SSTORE_SET_EXTRA, WARM_STORAGE_READ_COST,
    },
    ERGS_PER_GAS,
};
use ruint::aliases::U256;
use zk_ee::{
    common_structs::{history_map::CacheSnapshotId, EventContent, LogContent, WarmStorageKey},
    execution_environment_type::ExecutionEnvironmentType,
    kv_markers::MAX_EVENT_TOPICS,
    memory::stack_trait::{StackCtor, StackCtorConst},
    system::{errors::InternalError, logger::Logger, Resources, *},
    system_io_oracle::IOOracle,
    utils::{Bytes32, NopHasher},
};

use crate::system_implementation::{
    flat_storage_model::{FlatTreeWithAccountsUnderHashesStorageModel, *},
    system::public_input::{BlocksOutput, BlocksPublicInput, ChainStateCommitment},
};

mod io_subsystem;
mod public_input;

pub use self::{
    io_subsystem::*,
    public_input::{BatchOutput, BatchPublicInput},
};

#[derive(Clone, Copy, Debug, Default)]
pub struct EthereumLikeStorageAccessCostModel;

impl<R: Resources> StorageAccessPolicy<R, Bytes32> for EthereumLikeStorageAccessCostModel {
    fn charge_warm_storage_read(
        &self,
        ee_type: ExecutionEnvironmentType,
        resources: &mut R,
    ) -> Result<(), SystemError> {
        let ergs = match ee_type {
            ExecutionEnvironmentType::NoEE => Ergs::empty(),
            ExecutionEnvironmentType::EVM => Ergs(WARM_STORAGE_READ_COST * ERGS_PER_GAS),
            _ => return Err(InternalError("Unsupported EE").into()),
        };
        let native = R::Native::from_computational(
            crate::system_implementation::flat_storage_model::cost_constants::WARM_STORAGE_READ_NATIVE_COST,
        );
        resources.charge(&R::from_ergs_and_native(ergs, native))
    }

    fn charge_cold_storage_read_extra(
        &self,
        ee_type: ExecutionEnvironmentType,
        resources: &mut R,
        is_new_slot: bool,
        is_access_list: bool,
    ) -> Result<(), SystemError> {
        let ergs = match ee_type {
            ExecutionEnvironmentType::NoEE => {
                // Access list accesses are always done in NoEE.
                // Note that, for that reason, the warm cost isn't charged,
                // so here we charge full cold cost.
                if is_access_list {
                    Ergs(1900 * ERGS_PER_GAS)
                } else {
                    Ergs::empty()
                }
            }
            ExecutionEnvironmentType::EVM => {
                Ergs((COLD_SLOAD_COST - WARM_STORAGE_READ_COST) * ERGS_PER_GAS)
            }
            _ => return Err(InternalError("Unsupported EE").into()),
        };
        let native = if is_new_slot {
            R::Native::from_computational(
                crate::system_implementation::flat_storage_model::cost_constants::COLD_NEW_STORAGE_READ_NATIVE_COST,
            )
        } else {
            R::Native::from_computational(
            crate::system_implementation::flat_storage_model::cost_constants::COLD_EXISTING_STORAGE_READ_NATIVE_COST,)
        };
        resources.charge(&R::from_ergs_and_native(ergs, native))
    }

    fn charge_storage_write_extra(
        &self,
        ee_type: ExecutionEnvironmentType,
        initial_value: &Bytes32,
        current_value: &Bytes32,
        new_value: &Bytes32,
        resources: &mut R,
        is_warm_write: bool,
        is_new_slot: bool,
    ) -> Result<(), SystemError> {
        let ergs = match ee_type {
            ExecutionEnvironmentType::NoEE => Ergs::empty(),
            ExecutionEnvironmentType::EVM => {
                let total_cost = if new_value == current_value {
                    0
                } else if current_value == initial_value {
                    if initial_value.is_zero() {
                        // we do not purge slots, so we use another indicator here
                        SSTORE_SET_EXTRA
                    } else {
                        SSTORE_RESET_EXTRA
                    }
                } else {
                    0
                };

                let total_cost =
                    // In EVM spec there's a discrepancy for cold read and cold write costs. Cold
                    // writes add another 100 from thin air.
                    if is_warm_write == false { total_cost + 100 }
                    else { total_cost };

                Ergs(total_cost * ERGS_PER_GAS)
            }
            _ => return Err(InternalError("Unsupported EE").into()),
        };
        let native = if is_new_slot {
            R::Native::from_computational(
                crate::system_implementation::flat_storage_model::cost_constants::COLD_NEW_STORAGE_WRITE_EXTRA_NATIVE_COST,
            )
        } else {
            R::Native::from_computational(
          crate::system_implementation::flat_storage_model::cost_constants::COLD_EXISTING_STORAGE_WRITE_EXTRA_NATIVE_COST,)
        };
        resources.charge(&R::from_ergs_and_native(ergs, native))
    }
}

pub type ExtraCheck<SCC: const StackCtorConst, A: Allocator> =
    [[[[[[[(); SCC::extra_const_param::<(EventContent<MAX_EVENT_TOPICS, A>, ()), A>()];
        SCC::extra_const_param::<(LogContent<A>, u32), A>()];
        SCC::extra_const_param::<usize, A>()]; SCC::extra_const_param::<(usize, i32), A>()];
        SCC::extra_const_param::<CacheSnapshotId, A>()];
        SCC::extra_const_param::<Bytes32, A>()]; SCC::extra_const_param::<BitsOrd160, A>()];

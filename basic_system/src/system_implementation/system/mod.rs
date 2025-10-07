//! Implementation of the system interface.
use crate::system_implementation::cache_structs::storage_values::StorageAccessPolicy;
use crate::system_implementation::flat_storage_model::*;
use core::alloc::Allocator;
use errors::system::SystemError;
use evm_interpreter::gas_constants::COLD_SLOAD_COST;
use evm_interpreter::gas_constants::SSTORE_RESET_EXTRA;
use evm_interpreter::gas_constants::SSTORE_SET_EXTRA;
use evm_interpreter::gas_constants::WARM_STORAGE_READ_COST;
use evm_interpreter::ERGS_PER_GAS;
use ruint::aliases::U256;
use zk_ee::common_structs::history_map::CacheSnapshotId;
use zk_ee::common_structs::WarmStorageKey;
use zk_ee::execution_environment_type::ExecutionEnvironmentType;
use zk_ee::utils::Bytes32;
use zk_ee::{
    memory::stack_trait::StackCtor,
    oracle::IOOracle,
    storage_types::MAX_EVENT_TOPICS,
    system::{errors::internal::InternalError, logger::Logger, Resources, *},
};

mod basic_storage_model;
pub mod public_input;

pub use self::basic_storage_model::*;
pub use self::public_input::*;

#[derive(Clone, Copy, Debug, Default)]
pub struct EthereumLikeStorageAccessCostModel;

impl<R: Resources> StorageAccessPolicy<R, Bytes32> for EthereumLikeStorageAccessCostModel {
    fn charge_warm_storage_read(
        &self,
        ee_type: ExecutionEnvironmentType,
        resources: &mut R,
        is_access_list: bool,
    ) -> Result<(), SystemError> {
        let ergs = match ee_type {
            ExecutionEnvironmentType::NoEE => {
                // For access lists, EVM charges the full cost as many
                // times as a slot is in the list.
                if is_access_list {
                    Ergs(1900 * ERGS_PER_GAS)
                } else {
                    Ergs::empty()
                }
            }
            ExecutionEnvironmentType::EVM => Ergs(WARM_STORAGE_READ_COST * ERGS_PER_GAS),
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
    ) -> Result<(), SystemError> {
        let ergs = match ee_type {
            ExecutionEnvironmentType::NoEE => Ergs::empty(),
            ExecutionEnvironmentType::EVM => {
                Ergs((COLD_SLOAD_COST - WARM_STORAGE_READ_COST) * ERGS_PER_GAS)
            }
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

    /// Refund some resources if needed
    #[allow(unused_variables)]
    fn refund_for_storage_write(
        &self,
        ee_type: ExecutionEnvironmentType,
        value_at_tx_start: &Bytes32,
        current_value: &Bytes32,
        new_value: &Bytes32,
        resources: &mut R,
        refund_counter: &mut R,
    ) -> Result<(), SystemError> {
        if ee_type == ExecutionEnvironmentType::EVM {
            // EVM specific refunds calculation
            #[cfg(feature = "evm_refunds")]
            {
                if current_value != new_value {
                    if current_value == value_at_tx_start {
                        if !value_at_tx_start.is_zero() && new_value.is_zero() {
                            refund_counter.add_ergs(Ergs(4800 * ERGS_PER_GAS));
                        }
                    } else {
                        if !value_at_tx_start.is_zero() {
                            if current_value.is_zero() {
                                refund_counter.charge(&R::from_ergs(Ergs(4800 * ERGS_PER_GAS)))?;
                            } else if new_value.is_zero() {
                                refund_counter.add_ergs(Ergs(4800 * ERGS_PER_GAS));
                            }
                        }
                        if new_value == value_at_tx_start {
                            if value_at_tx_start.is_zero() {
                                refund_counter.add_ergs(Ergs((20000 - 100) * ERGS_PER_GAS));
                            } else {
                                refund_counter.add_ergs(Ergs((5000 - 2100 - 100) * ERGS_PER_GAS));
                            }
                        }
                    }
                }

                Ok(())
            }

            #[cfg(not(feature = "evm_refunds"))]
            {
                Ok(())
            }
        } else {
            Ok(())
        }
    }
}

//!
//! L2 base token system hook implementation.
//! It implements methods for `withdraw` and `withdrawWithMessage`,
//! which work in the same way as in Era.
//!
use crate::l1_messenger::send_to_l1_inner;

use super::*;
use core::fmt::Write;
use ruint::aliases::{B160, U256};
use zk_ee::execution_environment_type::ExecutionEnvironmentType;
use zk_ee::internal_error;
use zk_ee::system::errors::subsystem::SubsystemError;
use zk_ee::system::errors::{runtime::RuntimeError, system::SystemError};
use zk_ee::system::logger::Logger;

pub fn l2_base_token_hook<'a, S: EthereumLikeTypes>(
    request: ExternalCallRequest<S>,
    caller_ee: u8,
    system: &mut System<S>,
    return_memory: &'a mut [MaybeUninit<u8>],
) -> Result<(CompletedExecution<'a, S>, &'a mut [MaybeUninit<u8>]), SystemError>
where
    S::IO: IOSubsystemExt,
{
    let ExternalCallRequest {
        available_resources,
        ergs_to_pass: _,
        input: calldata,
        call_scratch_space: _,
        nominal_token_value,
        caller,
        callee,
        callers_caller: _,
        modifier,
    } = request;

    debug_assert_eq!(callee, L2_BASE_TOKEN_ADDRESS);

    let mut error = false;
    let mut is_static = false;
    match modifier {
        CallModifier::Constructor => {
            return Err(
                internal_error!("L2 base token hook called with constructor modifier").into(),
            )
        }
        CallModifier::Delegate
        | CallModifier::DelegateStatic
        | CallModifier::EVMCallcode
        | CallModifier::EVMCallcodeStatic => {
            error = true;
        }
        CallModifier::Static | CallModifier::ZKVMSystemStatic => {
            is_static = true;
        }
        _ => {}
    }

    if error {
        return Ok((make_error_return_state(available_resources), return_memory));
    }

    let mut resources = available_resources;

    let result = l2_base_token_hook_inner(
        &calldata,
        &mut resources,
        system,
        caller,
        caller_ee,
        nominal_token_value,
        is_static,
    );

    match result {
        Ok(Ok(return_data)) => Ok(make_return_state_from_returndata_region(
            resources,
            return_data,
        )),
        Ok(Err(e)) => {
            let _ = system
                .get_logger()
                .write_fmt(format_args!("Revert: {e:?}\n"));
            Ok(make_error_return_state(resources))
        }
        Err(SystemError::LeafRuntime(RuntimeError::OutOfErgs(_))) => {
            let _ = system
                .get_logger()
                .write_fmt(format_args!("Out of gas during system hook\n"));
            Ok(make_error_return_state(resources))
        }
        Err(e @ SystemError::LeafRuntime(RuntimeError::FatalRuntimeError(_))) => Err(e),
        Err(SystemError::LeafDefect(e)) => Err(e.into()),
    }
    .map(|x| (x, return_memory))
}

// withdraw(address) - 51cff8d9
pub const WITHDRAW_SELECTOR: &[u8] = &[0x51, 0xcf, 0xf8, 0xd9];

// withdrawWithMessage(address,bytes) - 84bc3eb0
pub const WITHDRAW_WITH_MESSAGE_SELECTOR: &[u8] = &[0x84, 0xbc, 0x3e, 0xb0];

// finalizeEthWithdrawal(uint256,uint256,uint16,bytes,bytes32[]) - 6c0960f9
pub const FINALIZE_ETH_WITHDRAWAL_SELECTOR: &[u8] = &[0x6c, 0x09, 0x60, 0xf9];

fn l2_base_token_hook_inner<S: EthereumLikeTypes>(
    calldata: &[u8],
    resources: &mut S::Resources,
    system: &mut System<S>,
    caller: B160,
    caller_ee: u8,
    nominal_token_value: U256,
    is_static: bool,
) -> Result<Result<&'static [u8], &'static str>, SystemError>
where
    S::IO: IOSubsystemExt,
{
    charge_native_and_proportional_gas::<S::Resources>(resources, HOOK_BASE_NATIVE_COST)?;

    if calldata.len() < 4 {
        return Ok(Err(
            "L2 base token failure: calldata shorter than selector length",
        ));
    }
    let mut selector = [0u8; 4];
    selector.copy_from_slice(&calldata[..4]);
    let _ = system
        .get_logger()
        .write_fmt(format_args!("Selector for l2 base token:"));
    let _ = system.get_logger().log_data(selector.iter().copied());
    // Calldata length shouldn't be able to overflow u32, due to gas
    // limitations.
    let calldata_len: u32 = calldata
        .len()
        .try_into()
        .map_err(|_| internal_error!("Calldata is larger than u32"))?;

    match selector {
        s if s == WITHDRAW_SELECTOR => {
            if is_static {
                return Ok(Err(
                    "L2 base token failure: withdraw called with static context",
                ));
            }
            // following solidity abi for withdraw(address)
            if calldata_len < 36 {
                return Ok(Err(
                    "L2 base token failure: withdraw called with invalid calldata",
                ));
            }
            // Burn nominal_token_value
            match system.io.update_account_nominal_token_balance(
                // Use EVM EE to charge for gas too
                ExecutionEnvironmentType::EVM,
                resources,
                &L2_BASE_TOKEN_ADDRESS,
                &nominal_token_value,
                true,
            ) {
                Ok(_) => Ok(()),
                // TODO this has to be properly propagated
                Err(SubsystemError::LeafUsage(_)) => Err(SystemError::LeafDefect(internal_error!(
                    "L2 base token must have withdrawal amount"
                ))),
                Err(SubsystemError::LeafRuntime(e)) => Err(e.into()),
                Err(SubsystemError::LeafDefect(e)) => Err(e.into()),
                Err(SubsystemError::Cascaded(e)) => match e {},
            }?;

            // Sending L2->L1 message.
            // ABI-encoded messages should consist of the following:
            // 32 bytes offset (must be 32)
            // 32 bytes length of the message
            // followed by the message itself, padded to be a multiple of 32 bytes.
            // In this case, it is known that the message is 56 bytes long:
            // - IMailbox.finalizeEthWithdrawal.selector (4)
            // - l1_receiver (20)
            // - nominal_token_value (32)

            // So the padded message will be 64 bytes long.
            // Total length of the encoded message will be 32 + 32 + 64 = 128 bytes.
            let mut l1_messenger_calldata = [0u8; 128];
            l1_messenger_calldata[31] = 32; // offset
            l1_messenger_calldata[63] = 56; // length
            l1_messenger_calldata[64..68].copy_from_slice(FINALIZE_ETH_WITHDRAWAL_SELECTOR);
            // check that first 12 bytes in address encoding are zero
            if calldata[4..4 + 12].iter().any(|byte| *byte != 0) {
                return Ok(Err(
                    "L2 base token failure: withdraw called with invalid calldata",
                ));
            }
            l1_messenger_calldata[68..88].copy_from_slice(&calldata[(4 + 12)..36]);
            l1_messenger_calldata[88..120]
                .copy_from_slice(&nominal_token_value.to_be_bytes::<32>());

            let result = send_to_l1_inner(
                &l1_messenger_calldata,
                resources,
                system,
                L2_BASE_TOKEN_ADDRESS,
                caller_ee,
            )?;

            Ok(result.map(|_| &[] as &[u8]))
        }
        s if s == WITHDRAW_WITH_MESSAGE_SELECTOR => {
            if is_static {
                return Ok(Err(
                    "L2 base token failure: withdrawWithMessage called with static context",
                ));
            }
            // following solidity abi for withdrawWithMessage(address,bytes)
            if calldata_len < 68 {
                return Ok(Err(
                    "L2 base token failure: withdrawWithMessage called with invalid calldata",
                ));
            }
            let message_offset: u32 =
                match U256::from_be_slice(&calldata[36..68]).try_into() {
                    Ok(offset) => offset,
                    Err(_) => return Ok(Err(
                        "L2 base token failure: withdrawWithMessage called with invalid calldata",
                    )),
                };
            // length located at 4+message_offset..4+message_offset+32
            // we want to check that 4+message_offset+32 will not overflow u32
            let length_encoding_end =
                match message_offset.checked_add(36) {
                    Some(length_encoding_end) => length_encoding_end,
                    None => return Ok(Err(
                        "L2 base token failure: withdrawWithMessage called with invalid calldata",
                    )),
                };
            if calldata_len < length_encoding_end {
                return Ok(Err(
                    "L2 base token failure: withdrawWithMessage called with invalid calldata",
                ));
            }
            let length: u32 =
                match U256::from_be_slice(
                    &calldata[(length_encoding_end as usize) - 32..length_encoding_end as usize],
                )
                .try_into()
                {
                    Ok(length) => length,
                    Err(_) => return Ok(Err(
                        "L2 base token failure: withdrawWithMessage called with invalid calldata",
                    )),
                };
            // to check that it will not overflow
            let message_end =
                match length_encoding_end.checked_add(length) {
                    Some(message_end) => message_end,
                    None => return Ok(Err(
                        "L2 base token failure: withdrawWithMessage called with invalid calldata",
                    )),
                };
            if calldata_len < message_end {
                return Ok(Err(
                    "L2 base token failure: withdrawWithMessage called with invalid calldata",
                ));
            }
            let additional_data = &calldata[(length_encoding_end as usize)..message_end as usize];

            // check that first 12 bytes in address encoding are zero
            if calldata[4..4 + 12].iter().any(|byte| *byte != 0) {
                return Ok(Err(
                    "L2 base token failure: withdrawWithMessage called with invalid calldata",
                ));
            }

            // Burn nominal_token_value
            match system.io.update_account_nominal_token_balance(
                // Use EVM EE to charge for gas too
                ExecutionEnvironmentType::EVM,
                resources,
                &L2_BASE_TOKEN_ADDRESS,
                &nominal_token_value,
                true,
            ) {
                Ok(_) => Ok(()),
                // TODO this has to be properly propagated
                Err(SubsystemError::LeafUsage(_)) => Err(SystemError::LeafDefect(internal_error!(
                    "L2 base token must have withdrawal amount"
                ))),
                Err(SubsystemError::LeafRuntime(e)) => Err(e.into()),
                Err(SubsystemError::LeafDefect(e)) => Err(e.into()),
                Err(SubsystemError::Cascaded(e)) => match e {},
            }?;

            // Sending L2->L1 message.
            // ABI-encoded messages should consist of the following:
            // 32 bytes offset (must be 32)
            // 32 bytes length of the message
            // followed by the message itself, padded to be a multiple of 32 bytes.
            // In this case, the message will consist of the following:
            // Packed ABI encoding of:
            // - IMailbox.finalizeEthWithdrawal.selector (4)
            // - l1_receiver (20)
            // - nominal_token_value (32)
            // - sender (20)
            // - additional_data (length of additional_data)
            let message_length = 76 + length;
            let abi_encoded_message_length = 32 + 32 + message_length;
            let abi_encoded_message_length = if abi_encoded_message_length % 32 != 0 {
                abi_encoded_message_length + (32 - (abi_encoded_message_length % 32))
            } else {
                abi_encoded_message_length
            };

            // First we charge for copying the message
            let native_copy_cost =
                evm_interpreter::native_resource_constants::COPY_BASE_NATIVE_COST.saturating_add(
                    evm_interpreter::native_resource_constants::COPY_BYTE_NATIVE_COST
                        .saturating_mul(abi_encoded_message_length as u64),
                );
            charge_native_and_proportional_gas::<S::Resources>(resources, native_copy_cost)?;

            let mut message: alloc::vec::Vec<u8, S::Allocator> = alloc::vec::Vec::with_capacity_in(
                abi_encoded_message_length as usize,
                system.get_allocator(),
            );

            // Offset and length
            message.extend_from_slice(&[0u8; 64]);
            message[31] = 32; // offset
            message[32..64].copy_from_slice(&U256::from(message_length).to_be_bytes::<32>());
            message.extend_from_slice(FINALIZE_ETH_WITHDRAWAL_SELECTOR);
            message.extend_from_slice(&calldata[16..36]);
            message.extend_from_slice(&nominal_token_value.to_be_bytes::<32>());
            message.extend_from_slice(&caller.to_be_bytes::<20>());
            message.extend_from_slice(additional_data);
            // Populating the rest of the message with zeros to make it a multiple of 32 bytes
            message.extend(core::iter::repeat_n(
                0u8,
                abi_encoded_message_length as usize - message.len(),
            ));

            let result = send_to_l1_inner(
                &message,
                resources,
                system,
                L2_BASE_TOKEN_ADDRESS,
                caller_ee,
            )?;

            Ok(result.map(|_| &[] as &[u8]))
        }
        _ => Ok(Err("L2 base token: unknown selector")),
    }
}

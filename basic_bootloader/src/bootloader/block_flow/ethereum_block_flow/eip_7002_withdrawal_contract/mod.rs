// NOTE: we implement 7002 contract as non-solidity/non-EMV contract as:
// - there is no GAS opcode in the reference bytecode
// - whatever will be the gas supplied to the frame - it'll be sufficient to pop as up to upper bound of elements
// - and to be honest, putting bytecode into execution client is so-so idea, and instead consensus can be instead reached on implementation
// Bytecode for this contract will anyway exist for requests creation in transactions themselves

use super::SSZ_BYTES_PER_LENGTH_OFFSET;
use ruint::aliases::B160;
use ruint::aliases::U256;
use zk_ee::execution_environment_type::ExecutionEnvironmentType;
use zk_ee::internal_error;
use zk_ee::system::errors::system::SystemError;
use zk_ee::system::AccountDataRequest;
use zk_ee::system::Computational;
use zk_ee::system::IOSubsystemExt;
use zk_ee::system::Resources;
use zk_ee::system::{errors::internal::InternalError, System};
use zk_ee::system::{EthereumLikeTypes, IOSubsystem};
use zk_ee::utils::{u256_to_usize_saturated, Bytes32};

pub const WITHDRAWAL_REQUEST_EIP_7685_TYPE: u8 = 0x01;

pub const WITHDRAWAL_REQUEST_PREDEPLOY_ADDRESS: B160 =
    B160::from_limbs([0xd83579A64c007002, 0xEf480Eb55e80D19a, 0x00000961]);

// const EXCESS_WITHDRAWAL_REQUESTS_STORAGE_SLOT: Bytes32 = Bytes32::from_hex("0000000000000000000000000000000000000000000000000000000000000000");
// const WITHDRAWAL_REQUEST_COUNT_STORAGE_SLOT: Bytes32 = Bytes32::from_hex("0000000000000000000000000000000000000000000000000000000000000001");
// const TARGET_WITHDRAWAL_REQUESTS_PER_BLOCK: usize = 2;

const WITHDRAWAL_REQUEST_QUEUE_HEAD_STORAGE_SLOT: Bytes32 =
    Bytes32::from_hex("0000000000000000000000000000000000000000000000000000000000000002");
const WITHDRAWAL_REQUEST_QUEUE_TAIL_STORAGE_SLOT: Bytes32 =
    Bytes32::from_hex("0000000000000000000000000000000000000000000000000000000000000003");
const WITHDRAWAL_REQUEST_QUEUE_STORAGE_OFFSET: U256 = U256::from_limbs([4, 0, 0, 0]);
const SLOTS_PER_REQUEST: U256 = U256::from_limbs([3, 0, 0, 0]);

const MAX_WITHDRAWAL_REQUESTS_PER_BLOCK: usize = 16;

// it's fully fixed
const WITHDRAWAL_REQUEST_SSZ_SERIALIZATION_LEN: usize = 20 + 48 + 8;

// NOTE: even though the spec says SSZ.encode (that is NOT a concatenation of element for the list), it actually appends nothing if there are no intercations
pub fn eip7002_system_part<S: EthereumLikeTypes>(
    system: &mut System<S>,
    requests_hasher: &mut impl crypto::sha256::Digest,
    // requests_hasher: &mut impl MiniDigest,
) -> Result<(), SystemError>
where
    S::IO: IOSubsystemExt,
{
    let mut resources = S::Resources::from_native(
        <S::Resources as Resources>::Native::from_computational(u64::MAX),
    );

    let props = resources.with_infinite_ergs(|resources| {
        system.io.read_account_properties(
            ExecutionEnvironmentType::NoEE,
            resources,
            &WITHDRAWAL_REQUEST_PREDEPLOY_ADDRESS,
            AccountDataRequest::empty()
                .with_nonce()
                .with_observable_bytecode_len(),
        )
    })?;

    let is_contract = props.nonce.0 == 1 && props.observable_bytecode_len.0 > 0;
    if is_contract == false {
        return Err(SystemError::LeafDefect(internal_error!(
            "EIP-7002 withdrawal contract is not deployed"
        )));
    }

    let queue_head_index = resources.with_infinite_ergs(|resources| {
        system.io.storage_read::<false>(
            ExecutionEnvironmentType::NoEE,
            resources,
            &WITHDRAWAL_REQUEST_PREDEPLOY_ADDRESS,
            &WITHDRAWAL_REQUEST_QUEUE_HEAD_STORAGE_SLOT,
        )
    })?;

    let queue_tail_index = resources.with_infinite_ergs(|resources| {
        system.io.storage_read::<false>(
            ExecutionEnvironmentType::NoEE,
            resources,
            &WITHDRAWAL_REQUEST_PREDEPLOY_ADDRESS,
            &WITHDRAWAL_REQUEST_QUEUE_TAIL_STORAGE_SLOT,
        )
    })?;

    let queue_head_index = U256::from_be_bytes(queue_head_index.as_u8_array());
    let queue_tail_index = U256::from_be_bytes(queue_tail_index.as_u8_array());

    let num_in_queue = queue_tail_index - queue_head_index;
    let num_dequeued = core::cmp::min(
        u256_to_usize_saturated(&num_in_queue),
        MAX_WITHDRAWAL_REQUESTS_PER_BLOCK,
    );

    if num_dequeued == 0 {
        // we do not even need to reset the queue poitners as it's a hard invariant
        assert!(queue_head_index.is_zero());
        assert!(queue_tail_index.is_zero());
        return Ok(());
    }

    // SSZ doesn't encode number of items in list (why make new format and avoid useful hints again?)

    requests_hasher.update([
        WITHDRAWAL_REQUEST_EIP_7685_TYPE,
        SSZ_BYTES_PER_LENGTH_OFFSET as u8,
        0,
        0,
        0,
    ]);

    for i in 0..num_dequeued {
        let queue_storage_slot = WITHDRAWAL_REQUEST_QUEUE_STORAGE_OFFSET
            + ((queue_head_index + U256::from(i as u64)) * SLOTS_PER_REQUEST);
        let slot_0 = Bytes32::from_array(queue_storage_slot.to_be_bytes::<32>());
        let slot_1 = Bytes32::from_array((queue_storage_slot + U256::from(1)).to_be_bytes::<32>());
        let slot_2 = Bytes32::from_array((queue_storage_slot + U256::from(2)).to_be_bytes::<32>());

        let slot_0 = resources.with_infinite_ergs(|resources| {
            system.io.storage_read::<false>(
                ExecutionEnvironmentType::NoEE,
                resources,
                &WITHDRAWAL_REQUEST_PREDEPLOY_ADDRESS,
                &slot_0,
            )
        })?;
        let slot_1 = resources.with_infinite_ergs(|resources| {
            system.io.storage_read::<false>(
                ExecutionEnvironmentType::NoEE,
                resources,
                &WITHDRAWAL_REQUEST_PREDEPLOY_ADDRESS,
                &slot_1,
            )
        })?;
        let slot_2 = resources.with_infinite_ergs(|resources| {
            system.io.storage_read::<false>(
                ExecutionEnvironmentType::NoEE,
                resources,
                &WITHDRAWAL_REQUEST_PREDEPLOY_ADDRESS,
                &slot_2,
            )
        })?;
        requests_hasher.update(&slot_0.as_u8_array_ref()[12..]);
        requests_hasher.update(slot_1.as_u8_array_ref());
        requests_hasher.update(&slot_2.as_u8_array_ref()[..(16 + 8)]);
    }

    let new_queue_head_index = queue_head_index + U256::from(num_dequeued as u64);
    if new_queue_head_index == queue_tail_index {
        resources.with_infinite_ergs(|resources| {
            system.io.storage_write::<false>(
                ExecutionEnvironmentType::NoEE,
                resources,
                &WITHDRAWAL_REQUEST_PREDEPLOY_ADDRESS,
                &WITHDRAWAL_REQUEST_QUEUE_HEAD_STORAGE_SLOT,
                &Bytes32::ZERO,
            )
        })?;

        resources.with_infinite_ergs(|resources| {
            system.io.storage_write::<false>(
                ExecutionEnvironmentType::NoEE,
                resources,
                &WITHDRAWAL_REQUEST_PREDEPLOY_ADDRESS,
                &WITHDRAWAL_REQUEST_QUEUE_TAIL_STORAGE_SLOT,
                &Bytes32::ZERO,
            )
        })?;
    } else {
        let value = Bytes32::from_array(new_queue_head_index.to_be_bytes::<32>());
        let _ = resources.with_infinite_ergs(|resources| {
            system.io.storage_write::<false>(
                ExecutionEnvironmentType::NoEE,
                resources,
                &WITHDRAWAL_REQUEST_PREDEPLOY_ADDRESS,
                &WITHDRAWAL_REQUEST_QUEUE_HEAD_STORAGE_SLOT,
                &value,
            )
        })?;
    }

    Ok(())
}

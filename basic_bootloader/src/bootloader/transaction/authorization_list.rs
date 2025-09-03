//!
//! Parser and logic for authorization lists.
//! See [ZkSyncTransaction] for more details on encoding format.
//!

use core::fmt::Write;

use crate::bootloader::rlp;
use crate::bootloader::transaction::reserved_dynamic_parser::{
    parse_address, parse_u256, parse_u32, parse_u64, parse_u8,
};
use crate::bootloader::BootloaderSubsystemError;
use evm_interpreter::ERGS_PER_GAS;
use ruint::aliases::{B160, U256};
use zk_ee::execution_environment_type::ExecutionEnvironmentType;
use zk_ee::memory::ArrayBuilder;
use zk_ee::system::errors::interface::InterfaceError;
use zk_ee::system::errors::subsystem::SubsystemError;
use zk_ee::system::errors::system::SystemError;
use zk_ee::system::{AccountDataRequest, EthereumLikeTypes, IOSubsystemExt, Resources, System};
use zk_ee::system::{NonceError, Resource};
use zk_ee::{internal_error, wrap_error};

use super::TxError;

#[derive(Clone, Copy, Debug)]
pub struct AuthorizationListParser {
    pub offset: usize,
}

impl AuthorizationListParser {
    pub fn into_iter<'a>(&self, slice: &'a [u8]) -> Result<AuthorizationListIter<'a>, ()> {
        AuthorizationListIter::new(slice, self.offset)
    }
}

pub struct AuthorizationListIter<'a> {
    slice: &'a [u8],
    pub(crate) count: usize,
    head_start: usize,
    index: usize,
}

const AUTHORIZATION_LIST_ITEM_BYTES: usize = 6 * 32;
pub struct AuthorizationListItem {
    pub chain_id: U256,
    pub address: B160,
    pub nonce: u64,
    pub y_parity: u8,
    pub r: U256,
    pub s: U256,
}

impl<'a> AuthorizationListIter<'a> {
    pub fn empty(slice: &'a [u8]) -> Self {
        // Offset doesn't matter here, as we first check if it's empty
        Self {
            slice,
            count: 0,
            head_start: 0,
            index: 0,
        }
    }

    fn new(slice: &'a [u8], offset: usize) -> Result<Self, ()> {
        let count = parse_u32(slice, offset)?;
        let head_start = offset + 32;

        Ok(AuthorizationListIter {
            slice,
            count,
            head_start,
            index: 0,
        })
    }

    fn parse_current(&mut self) -> Result<AuthorizationListItem, ()> {
        // Assume index < count, checked by iterator impl
        let offset = self.head_start
            + self
                .index
                .checked_mul(AUTHORIZATION_LIST_ITEM_BYTES)
                .ok_or(())?;
        let chain_id = parse_u256(&self.slice, offset)?;
        let address = parse_address(&self.slice, offset + 32)?;
        let nonce = parse_u64(&self.slice, offset + 2 * 32)?;
        let y_parity = parse_u8(&self.slice, offset + 3 * 32)?;
        let r = parse_u256(&self.slice, offset + 4 * 32)?;
        let s = parse_u256(&self.slice, offset + 5 * 32)?;
        Ok(AuthorizationListItem {
            chain_id,
            address,
            nonce,
            y_parity,
            r,
            s,
        })
    }
}

impl<'a> Iterator for AuthorizationListIter<'a> {
    type Item = Result<AuthorizationListItem, ()>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.index >= self.count {
            return None;
        }
        let current = self.parse_current();
        self.index += 1;
        Some(current)
    }
}

impl AuthorizationListItem {
    /// Validate and apply an authorization list item, following EIP-7702:
    /// 1. Verify the chain ID is 0 or the ID of the current chain.
    /// 2. Verify the nonce is less than 2**64 - 1.
    /// 3. Let authority = ecrecover(msg, y_parity, r, s).
    ///    Where msg = keccak(MAGIC || rlp([chain_id, address, nonce])).
    ///    Verify s is less than or equal to secp256k1n/2.
    /// 4. Warm up authority
    /// 5. Verify the authority is not a contract.
    /// 6. Verify the nonce of authority is equal to nonce.
    /// 7. Add refund if authority isn't empty.
    /// 8. Set the code of authority to be 0xef0100 || address.
    ///    If address is 0x0, clear the account’s code
    ///    (and deployment status) instead.
    /// 9. Increase the nonce of authority by one.
    ///
    /// Note that if any of these checks fail, the function returns
    /// false.
    pub(crate) fn validate_and_apply_delegation<S: EthereumLikeTypes>(
        self,
        system: &mut System<S>,
        resources: &mut S::Resources,
    ) -> Result<bool, TxError>
    where
        S::IO: IOSubsystemExt,
    {
        use zk_ee::system::Ergs;

        // pre-charge
        resources.charge(&S::Resources::from_ergs_and_native(
            Ergs(evm_interpreter::gas_constants::NEWACCOUNT * ERGS_PER_GAS),
                <<S::Resources as Resources>::Native as zk_ee::system::Computational>::from_computational(crate::bootloader::constants::PER_AUTH_INTRINSIC_COST)
            )
        )?;

        let chain_id = system.get_chain_id();
        // 1. Check chain id
        if !self.chain_id.is_zero() && self.chain_id != U256::from(chain_id) {
            return Ok(false);
        }
        // 2. Check for nonce overflow
        if self.nonce == u64::MAX {
            return Ok(false);
        }
        // 3. Signature
        // EIP-2 check
        if self.s > U256::from_be_bytes(crypto::secp256k1::SECP256K1N_HALF) {
            return Ok(false);
        }
        let msg = resources.with_infinite_ergs(|inf_ergs| self.compute_message::<S>(inf_ergs))?;
        let Some(authority) =
            resources.with_infinite_ergs(|inf_ergs| self.recover(system, inf_ergs, msg))?
        else {
            return Ok(false);
        };

        // 4. Read authority account
        let account_properties = resources.with_infinite_ergs(|inf_ergs| {
            system.io.read_account_properties(
                ExecutionEnvironmentType::NoEE,
                inf_ergs,
                &authority,
                AccountDataRequest::empty()
                    .with_nonce()
                    .with_has_bytecode()
                    .with_is_delegated()
                    .with_nominal_token_balance(),
            )
        })?;
        // 5. Check authority is not a contract
        if account_properties.is_contract() {
            return Ok(false);
        }
        // 6. Check nonce
        if account_properties.nonce.0 != self.nonce {
            return Ok(false);
        }
        // 7. Add refund if authority is not empty.
        #[cfg(feature = "evm_refunds")]
        {
            let is_empty = account_properties.nonce.0 == 0
                && account_properties.has_bytecode.0 == false
                && account_properties.nominal_token_balance.0.is_zero();
            if !is_empty {
                system
                    .io
                    .add_to_refund_counter(S::Resources::from_ergs(Ergs(
                        (evm_interpreter::gas_constants::NEWACCOUNT
                            - evm_interpreter::gas_constants::PER_AUTH_BASE_COST)
                            * ERGS_PER_GAS,
                    )))?;
            }
        }

        let _ = system.get_logger().write_fmt(format_args!(
            "Will delegate address 0x{:040x} -> 0x{:040x}\n",
            authority.as_uint(),
            self.address.as_uint()
        ));

        // 8. Set code for authority, system function
        //    will handle the two cases (unsetting).
        resources.with_infinite_ergs(|inf_ergs| {
            system
                .io
                .set_delegation(inf_ergs, &authority, &self.address)
        })?;
        // 9.Bump nonce
        resources
            .with_infinite_ergs(|inf_ergs| {
                system
                    .io
                    .increment_nonce(ExecutionEnvironmentType::NoEE, inf_ergs, &authority, 1)
            })
            .map_err(|e| -> BootloaderSubsystemError {
                match e {
                    SubsystemError::LeafUsage(InterfaceError(NonceError::NonceOverflow, _)) => {
                        internal_error!("Cannot overflow, already checked").into()
                    }
                    _ => wrap_error!(e),
                }
            })?;
        Ok(true)
    }

    fn compute_message<S: EthereumLikeTypes>(
        &self,
        resources: &mut S::Resources,
    ) -> Result<[u8; 32], TxError> {
        use crate::bootloader::transaction::EIP7702_MAGIC;
        use crypto::sha3::Keccak256;
        use crypto::MiniDigest;

        let list_payload_len =
            rlp::estimate_number_encoding_len(&self.chain_id.to_be_bytes::<32>())
                + rlp::ADDRESS_ENCODING_LEN
                + rlp::estimate_number_encoding_len(&self.nonce.to_be_bytes());
        let total_list_len = rlp::estimate_length_encoding_len(list_payload_len) + list_payload_len;
        let encoding_len = 1 + total_list_len;
        crate::bootloader::transaction::charge_keccak(encoding_len, resources)?;
        let mut hasher = Keccak256::new();
        hasher.update([EIP7702_MAGIC]);
        rlp::apply_list_length_encoding_to_hash(list_payload_len, &mut hasher);
        rlp::apply_number_encoding_to_hash(&self.chain_id.to_be_bytes::<32>(), &mut hasher);
        rlp::apply_bytes_encoding_to_hash(
            &self.address.to_be_bytes::<{ B160::BYTES }>(),
            &mut hasher,
        );
        rlp::apply_number_encoding_to_hash(&self.nonce.to_be_bytes(), &mut hasher);
        Ok(hasher.finalize())
    }

    fn recover<S: EthereumLikeTypes>(
        &self,
        system: &mut System<S>,
        resources: &mut S::Resources,
        msg: [u8; 32],
    ) -> Result<Option<B160>, TxError> {
        use zk_ee::system::SystemFunctions;
        let mut ecrecover_input = [0u8; 128];
        ecrecover_input[0..32].copy_from_slice(&msg);
        ecrecover_input[63] = if self.y_parity <= 1 {
            self.y_parity + 27
        } else {
            self.y_parity
        };
        ecrecover_input[64..96].copy_from_slice(&self.r.to_be_bytes::<32>());
        ecrecover_input[96..128].copy_from_slice(&self.s.to_be_bytes::<32>());
        let mut ecrecover_output = ArrayBuilder::default();
        // Recover is counted in intrinsic gas
        resources
            .with_infinite_ergs(|inf_ergs| {
                S::SystemFunctions::secp256k1_ec_recover(
                    ecrecover_input.as_slice(),
                    &mut ecrecover_output,
                    inf_ergs,
                    system.get_allocator(),
                )
            })
            .map_err(SystemError::from)?;
        if ecrecover_output.is_empty() {
            Ok(None)
        } else {
            Ok(Some(
                B160::try_from_be_slice(&ecrecover_output.build()[12..])
                    .ok_or(internal_error!("Invalid ecrecover return value"))?,
            ))
        }
    }
}

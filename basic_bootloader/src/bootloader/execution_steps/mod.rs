pub(crate) mod execute;
pub(crate) mod perform_deployment;
pub(crate) mod process_fee_payments;
pub(crate) mod validate;

use crate::bootloader::gas_helpers::ResourcesForTx;
use ruint::aliases::B160;
use ruint::aliases::U256;
use zk_ee::system::ReturnValues;
use zk_ee::system::SystemTypes;
use zk_ee::system::{Ergs, EthereumLikeTypes};
use zk_ee::utils::Bytes32;

pub(crate) struct TxContextForPreAndPostProcessing<S: EthereumLikeTypes> {
    pub(crate) resources: ResourcesForTx<S>,
    pub(crate) fee_to_prepay: U256,
    pub(crate) gas_price_to_use: U256,
    pub(crate) minimal_ergs_to_charge: Ergs,
    pub(crate) originator_nonce_to_use: u64,
    pub(crate) tx_hash: Bytes32,
    pub(crate) native_per_pubdata: U256,
    pub(crate) native_per_gas: U256,
}

// Address deployed, or reason for the lack thereof.
pub(crate) enum DeployedAddress {
    CallNoAddress,
    RevertedNoAddress,
    Address(B160),
}

pub(crate) struct TxExecutionResult<'a, S: SystemTypes> {
    pub(crate) return_values: ReturnValues<'a, S>,
    pub(crate) resources_returned: S::Resources,
    pub(crate) reverted: bool,
    pub(crate) deployed_address: DeployedAddress,
}

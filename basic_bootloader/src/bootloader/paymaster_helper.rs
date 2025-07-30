use super::transaction::ZkSyncTransaction;
use super::*;
use constants::TX_CALLDATA_OFFSET;
use errors::BootloaderSubsystemError;
use system_hooks::addresses_constants::BOOTLOADER_FORMAL_ADDRESS;
use system_hooks::HooksStorage;
use zk_ee::internal_error;
use zk_ee::system::{EthereumLikeTypes, System};

// Helpers for paymaster flow.

impl<S: EthereumLikeTypes> BasicBootloader<S> {
    fn write_calldata_prefix(
        pre_tx_buffer: &mut [u8],
        calldata_start: usize,
        selector: &[u8],
        tx_hash: Bytes32,
        suggested_signed_hash: Bytes32,
    ) {
        // Write selector
        pre_tx_buffer[calldata_start..(calldata_start + 4)].copy_from_slice(selector);
        // Write tx_hash
        let tx_hash_start = calldata_start + 4;
        pre_tx_buffer[tx_hash_start..(tx_hash_start + U256::BYTES)]
            .copy_from_slice(tx_hash.as_u8_ref());
        // Write suggested_signed_hash
        let signed_start = tx_hash_start + U256::BYTES;
        pre_tx_buffer[signed_start..(signed_start + U256::BYTES)]
            .copy_from_slice(suggested_signed_hash.as_u8_ref());
        // Write offset
        let offset_start = signed_start + U256::BYTES;
        pre_tx_buffer[offset_start..(offset_start + U256::BYTES)]
            .copy_from_slice(U256::to_be_bytes::<32>(&U256::from(TX_CALLDATA_OFFSET)).as_ref());
    }

    /// Used to call a method with the following signature;
    /// someName(
    ///     bytes32 _txHash,
    ///     bytes32 _suggestedSignedHash,
    ///     Transaction calldata _transaction
    /// )
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::type_complexity)]
    pub fn call_account_method<'a>(
        system: &mut System<S>,
        system_functions: &mut HooksStorage<S, S::Allocator>,
        memories: RunnerMemoryBuffers<'a>,
        transaction: &mut ZkSyncTransaction,
        tx_hash: Bytes32,
        suggested_signed_hash: Bytes32,
        from: B160,
        selector: &[u8],
        resources: &mut S::Resources,
        tracer: &mut impl Tracer<S>,
    ) -> Result<CompletedExecution<'a, S>, BootloaderSubsystemError>
    where
        S::IO: IOSubsystemExt,
    {
        let header_length = 4 + U256::BYTES * 3;
        let calldata_start = TX_OFFSET - header_length;
        let calldata_end = calldata_start
            .checked_add(transaction.tx_body_length())
            .ok_or(internal_error!("overflow"))?;

        let pre_tx_buffer = transaction.pre_tx_buffer();
        Self::write_calldata_prefix(
            pre_tx_buffer,
            calldata_start,
            selector,
            tx_hash,
            suggested_signed_hash,
        );

        // we can now take and cast as transaction is static relative to EEs
        let calldata = &transaction.underlying_buffer()[calldata_start..calldata_end];

        let resources_for_tx = resources.clone();

        BasicBootloader::run_single_interaction(
            system,
            system_functions,
            memories,
            calldata,
            &BOOTLOADER_FORMAL_ADDRESS,
            &from,
            resources_for_tx,
            &U256::ZERO,
            true,
            tracer,
        )
    }
}

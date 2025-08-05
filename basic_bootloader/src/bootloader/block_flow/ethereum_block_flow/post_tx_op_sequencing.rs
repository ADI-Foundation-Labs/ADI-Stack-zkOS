use basic_system::system_implementation::ethereum_storage_model::EthereumStorageModel;

use super::*;

impl<
        A: Allocator + Clone + Default,
        R: Resources,
        P: StorageAccessPolicy<R, Bytes32> + Default,
        SC: StackCtor<N>,
        O: IOOracle,
        const N: usize,
        S: EthereumLikeTypes<
            IO = BasicStorageModel<
                A,
                R,
                P,
                SC,
                N,
                O,
                EthereumStorageModel<A, R, P, SC, N, false>,
                false,
            >,
        >,
    > PostTxLoopOp<S> for EthereumPostOp<false>
where
    S::IO: IOSubsystemExt + IOTeardown<S::IOTypes>,
{
    type BlockData = EthereumBasicTransactionDataKeeper;
    type PostTxLoopOpResult = ();

    fn post_op(
        system: System<S>,
        block_data: Self::BlockData,
        result_keeper: &mut impl ResultKeeperExt<EthereumIOTypesConfig>,
    ) -> Self::PostTxLoopOpResult {
        // Here we have to cascade everything

        let mut logger = system.get_logger();

        // TODO: we need formal header for now, but later on restructure it

        let block_gas_used = block_data.block_gas_used;
        let block_number = system.get_block_number();
        let previous_block_hash = system.get_blockhash(block_number);
        let beneficiary = system.get_coinbase();
        let gas_limit = system.get_gas_limit();
        let timestamp = system.get_timestamp();
        let consensus_random = Bytes32::from_u256_be(&system.get_mix_hash());
        let base_fee_per_gas = system.get_eip1559_basefee();

        // TODO: check if it's indeed u64
        let base_fee_per_gas = base_fee_per_gas.try_into().unwrap();
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

        let System {
            mut io, metadata, ..
        } = system;

        // Storage

        // 0. Flush accounts into storage, report preimages if needed
        io.flush_caches(result_keeper);
        io.report_new_preimages(result_keeper);

        // These two benefit from filter being applied early, so for now it's kept using internal structure
        result_keeper.basic_account_diffs(io.storage.account_cache.net_diffs_iter());
        result_keeper.storage_diffs(io.storage.storage_cache.net_diffs_iter().map(|(k, v)| {
            let WarmStorageKey { address, key } = k;
            let value = v.current_value;
            (address, key, value)
        }));

        // Events
        result_keeper.events(io.events_iterator());

        let mut initial_state_commitment = {
            use zk_ee::system_io_oracle::IOOracle;
            use zk_ee::system_io_oracle::INITIAL_STATE_COMMITTMENT_QUERY_ID;

            io.oracle()
                .query_with_empty_input(INITIAL_STATE_COMMITTMENT_QUERY_ID)
                .unwrap()
        };
        let _ = logger.write_fmt(format_args!(
            "Initial state commitment is {:?}\n",
            &initial_state_commitment
        ));

        // // 3. Verify/apply reads and writes
        cycle_marker::wrap!("verify_and_apply_batch", {
            io.update_commitment(Some(&mut initial_state_commitment), &mut logger);
        });

        ()
    }
}

use super::*;
use crate::bootloader::block_flow::zk_block_flow::batch_pi_builder::BatchPublicInputBuilder;
use zk_ee::metadata_markers::basic_metadata::BasicBlockMetadata;

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
                FlatTreeWithAccountsUnderHashesStorageModel<A, R, P, SC, N, true>,
                true,
            >,
        >,
    > PostTxLoopOp<S> for ZKHeaderStructurePostTxOpProvingMultiblockBatch
where
    S::IO: IOSubsystemExt
        + IOTeardown<S::IOTypes, IOStateCommitment = FlatStorageCommitment<TREE_HEIGHT>>, // IOStateCommitment bound is trivial, most likely needed due to missing associated types equality feature in the current state of the compiler
{
    type BlockData = ZKBasicTransactionDataKeeper<NopTxHashesAccumulator>;
    type BatchData = BatchPublicInputBuilder;
    type PostTxLoopOpResult = O;
    type BlockHeader = crate::bootloader::block_header::BlockHeader;

    fn post_op(
        system: System<S>,
        block_data: Self::BlockData,
        batch_data: &mut Self::BatchData,
        result_keeper: &mut impl ResultKeeperExt<EthereumIOTypesConfig>,
    ) -> Self::PostTxLoopOpResult {
        // form block header
        let tx_rolling_hash = block_data.transaction_hashes_accumulator.finish();

        let upgrade_tx_hash = block_data.upgrade_tx_recorder.finish();
        let block_gas_used = block_data.block_gas_used;

        let block_number = system.get_block_number();
        let previous_block_hash = if block_number == 0 {
            Bytes32::ZERO
        } else {
            system.get_blockhash(block_number - 1)
        };
        let beneficiary = system.get_coinbase();
        // TODO: Gas limit should be constant
        let gas_limit = system.get_gas_limit();
        // TODO: gas used shouldn't be zero
        let timestamp = system.get_timestamp();
        let consensus_random = system.get_mix_hash();
        let base_fee_per_gas = system.get_eip1559_basefee();
        // TODO: add gas_per_pubdata and native price

        // TODO: check if it's indeed u64
        let base_fee_per_gas = base_fee_per_gas.try_into().unwrap();
        // let base_fee_per_gas = base_fee_per_gas
        //     .try_into()
        //     .map_err(|_| internal_error!("base_fee_per_gas exceeds max u64"))?;
        let block_header = BlockHeader::new(
            previous_block_hash,
            beneficiary,
            tx_rolling_hash,
            block_number,
            gas_limit,
            block_gas_used,
            timestamp,
            consensus_random,
            base_fee_per_gas,
        );
        let current_block_hash = Bytes32::from(block_header.hash());
        result_keeper.block_sealed(block_header);

        // then perform IO related part

        let mut logger = system.get_logger();
        let _ = logger.write_fmt(format_args!("Basic header information was created\n"));

        let System {
            mut io, metadata, ..
        } = system;

        let (mut state_commitment, last_block_timestamp): (
            FlatStorageCommitment<TREE_HEIGHT>,
            u64,
        ) = {
            use zk_ee::basic_queries::ZKProofDataQuery;
            use zk_ee::common_structs::ProofData;
            use zk_ee::system_io_oracle::SimpleOracleQuery;

            let proof_data: ProofData<FlatStorageCommitment<TREE_HEIGHT>> =
                ZKProofDataQuery::get(io.oracle(), &()).unwrap();

            (proof_data.state_root_view, proof_data.last_block_timestamp)
        };

        let _ = logger.write_fmt(format_args!(
            "Initial state commitment is {:?}\n",
            &state_commitment
        ));

        let mut blocks_hasher = crypto::blake2s::Blake2s256::new();
        for depth in 0..256 {
            blocks_hasher.update(
                metadata
                    .block_historical_hash(depth)
                    .expect("must be known for such depth")
                    .as_u8_ref(),
            );
        }

        use basic_system::system_implementation::system::public_input::ChainStateCommitment;

        // chain state before
        let chain_state_commitment_before = ChainStateCommitment {
            state_root: state_commitment.root,
            next_free_slot: state_commitment.next_free_slot,
            block_number: metadata.block_number() - 1,
            last_256_block_hashes_blake: blocks_hasher.finalize().into(),
            last_block_timestamp,
        };
        let _ = logger.write_fmt(format_args!(
            "PI calculation: state commitment before {:?}\n",
            chain_state_commitment_before
        ));

        batch_data
            .pubdata_hasher
            .update(current_block_hash.as_u8_ref());

        result_keeper.pubdata(current_block_hash.as_u8_ref());

        // Storage

        // 0. Flush accounts into storage, report preimages if needed
        io.flush_caches(result_keeper);
        io.report_new_preimages(result_keeper);

        // 1. Return uncompressed NET state diffs for sequencer
        // It benefit from filter being applied early, so for now it's kept using internal structure
        result_keeper.storage_diffs(io.storage.storage_cache.net_diffs_iter().map(|(k, v)| {
            let WarmStorageKey { address, key } = k;
            let value = v.current_value;
            (address, key, value)
        }));

        // 2. Commit to/return compressed pubdata
        io.storage.apply_storage_diffs_pubdata(
            result_keeper,
            &mut batch_data.pubdata_hasher,
            &mut io.oracle,
        );

        // Logs pubdata
        // use concrete type as it's non-trivial
        let log_record_fn = |log_hash: &Bytes32| {
            batch_data.logs_storage.try_push(*log_hash).unwrap();
        };
        io.logs_storage.apply_logs_to_pubdata_and_record_log_hashes(
            result_keeper,
            &mut batch_data.pubdata_hasher,
            Some(log_record_fn),
        );
        // Logs themselves
        // TODO: why messages?
        result_keeper.logs(io.logs_storage.messages_ref_iter());

        // Events - no pubdata
        result_keeper.events(io.events_storage.events_ref_iter());

        // 3. Verify/apply reads and writes
        cycle_marker::wrap!("verify_and_apply_batch", {
            IOTeardown::<_>::update_commitment(
                &mut io,
                Some(&mut state_commitment),
                &mut logger,
                result_keeper,
            );
        });

        let mut blocks_hasher = crypto::blake2s::Blake2s256::new();
        blocks_hasher.update(current_block_hash.as_u8_ref());
        for depth in 0..255 {
            blocks_hasher.update(
                metadata
                    .block_historical_hash(depth)
                    .expect("must be known for such depth")
                    .as_u8_ref(),
            );
        }

        // validate that timestamp didn't decrease
        assert!(metadata.block_timestamp() >= last_block_timestamp);

        let block_number = metadata.block_number();
        let block_timestamp = metadata.block_timestamp();

        // chain state after
        let chain_state_commitment_after = ChainStateCommitment {
            state_root: state_commitment.root,
            next_free_slot: state_commitment.next_free_slot,
            block_number,
            last_256_block_hashes_blake: blocks_hasher.finalize().into(),
            last_block_timestamp: block_timestamp,
        };
        let _ = logger.write_fmt(format_args!(
            "PI calculation: state commitment after {:?}\n",
            chain_state_commitment_after
        ));

        batch_data.apply_block(
            chain_state_commitment_before.hash().into(),
            chain_state_commitment_after.hash().into(),
            block_timestamp,
            U256::from(metadata.chain_id()),
            upgrade_tx_hash,
        );

        io.oracle
    }
}

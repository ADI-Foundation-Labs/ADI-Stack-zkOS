use crate::system_implementation::flat_storage_model::TREE_HEIGHT;
use crate::system_implementation::system::ChainStateCommitment;
use crate::system_implementation::system::FlatStorageCommitment;
use crate::system_implementation::system::TypedFinishIO;
use zk_ee::system::logger::Logger;
use zk_ee::system::{IOResultKeeper, IOSubsystemExt, System, SystemTypes};
use zk_ee::system_io_oracle::IOOracle;
use zk_ee::types_config::EthereumIOTypesConfig;
use zk_ee::utils::Bytes32;
use zk_ee::utils::NopHasher;

pub trait SystemPostWork<S: SystemTypes>
where
    S::IO: IOSubsystemExt + TypedFinishIO,
{
    type FinalData;

    fn finish(
        self,
        system: System<S>,
        result_keeper: &mut impl IOResultKeeper<S::IOTypes>,
        logger: &mut impl Logger,
    ) -> Self::FinalData;
}

pub struct DefaultHeaderStructurePostWork<const PROOF_ENV: bool> {
    pub current_block_hash: Bytes32,
    pub l1_to_l2_txs_hash: Bytes32,
    pub upgrade_tx_hash: Bytes32,
}

impl<S: SystemTypes<IOTypes = EthereumIOTypesConfig>> SystemPostWork<S>
    for DefaultHeaderStructurePostWork<false>
where
    S::IO: IOSubsystemExt + TypedFinishIO<FinalData = <S::IO as IOSubsystemExt>::IOOracle>,
{
    type FinalData = <S::IO as IOSubsystemExt>::IOOracle;

    fn finish(
        self,
        system: System<S>,
        result_keeper: &mut impl IOResultKeeper<EthereumIOTypesConfig>,
        logger: &mut impl Logger,
    ) -> Self::FinalData {
        result_keeper.pubdata(self.current_block_hash.as_u8_ref());

        system
            .io
            .finish(None, &mut NopHasher, &mut NopHasher, result_keeper, logger)
    }
}

// In practice we will not use single block batches
// This functionality is here only for the tests
#[cfg(not(feature = "wrap-in-batch"))]
impl<S: SystemTypes<IOTypes = EthereumIOTypesConfig>> SystemPostWork<S>
    for DefaultHeaderStructurePostWork<true>
where
    S::IO: IOSubsystemExt
        + TypedFinishIO<
            FinalData = <S::IO as IOSubsystemExt>::IOOracle,
            IOStateCommittment = FlatStorageCommitment<TREE_HEIGHT>,
        >,
{
    type FinalData = (<S::IO as IOSubsystemExt>::IOOracle, Bytes32);

    fn finish(
        self,
        system: System<S>,
        result_keeper: &mut impl IOResultKeeper<EthereumIOTypesConfig>,
        logger: &mut impl Logger,
    ) -> Self::FinalData {
        use crypto::blake2s::Blake2s256;
        use crypto::MiniDigest;

        // let initial_state_commitment = {
        //     use zk_ee::system_io_oracle::INITIAL_STATE_COMMITTMENT_QUERY_ID;

        //     system.io.oracle()
        //         .query_with_empty_input::<<S::IO as TypedFinishIO>::IOStateCommittment>(INITIAL_STATE_COMMITTMENT_QUERY_ID)
        //         .unwrap()
        // };
        // let mut state_commitment = initial_state_commitment.clone();

        let System {
            mut io, metadata, ..
        } = system;

        let mut state_commitment = {
            // TODO (EVM-989): read only state commitment
            use zk_ee::common_structs::BasicIOImplementerFSM;
            use zk_ee::system_io_oracle::INITIALIZE_IO_IMPLEMENTER_QUERY_ID;

            let fsm_state: BasicIOImplementerFSM<FlatStorageCommitment<TREE_HEIGHT>> = io
                .oracle()
                .query_with_empty_input(INITIALIZE_IO_IMPLEMENTER_QUERY_ID)
                .unwrap();

            fsm_state.state_root_view
        };

        result_keeper.pubdata(self.current_block_hash.as_u8_ref());

        let mut blocks_hasher = Blake2s256::new();
        for block_hash in metadata.block_level_metadata.block_hashes.0.iter() {
            blocks_hasher.update(&block_hash.to_be_bytes::<32>());
        }

        // chain state before
        let chain_state_commitment_before = ChainStateCommitment {
            state_root: state_commitment.root,
            next_free_slot: state_commitment.next_free_slot,
            block_number: metadata.block_level_metadata.block_number - 1,
            last_256_block_hashes_blake: blocks_hasher.finalize().into(),
            // TODO(EVM-1080): we should set and validate that current block timestamp >= previous
            last_block_timestamp: 0,
        };

        let mut pubdata_hasher = Blake2s256::new();
        pubdata_hasher.update(self.current_block_hash.as_u8_ref());
        let mut l2_to_l1_logs_hasher = Blake2s256::new();

        // finishing IO, applying changes
        let io_final_data = io.finish(
            Some(&mut state_commitment),
            &mut l2_to_l1_logs_hasher,
            &mut pubdata_hasher,
            result_keeper,
            logger,
        );

        let pubdata_hash = pubdata_hasher.finalize();
        let l2_to_l1_logs_hashes_hash = l2_to_l1_logs_hasher.finalize();

        // rehash
        let mut blocks_hasher = Blake2s256::new();
        for block_hash in metadata.block_level_metadata.block_hashes.0.iter().skip(1) {
            blocks_hasher.update(&block_hash.to_be_bytes::<32>());
        }
        blocks_hasher.update(self.current_block_hash.as_u8_ref());

        // chain state after
        let chain_state_commitment_after = ChainStateCommitment {
            state_root: state_commitment.root,
            next_free_slot: state_commitment.next_free_slot,
            block_number: metadata.block_level_metadata.block_number,
            last_256_block_hashes_blake: blocks_hasher.finalize().into(),
            // TODO(EVM-1080): we should set and validate that current block timestamp >= previous
            last_block_timestamp: 0,
        };

        use crate::system_implementation::system::BlocksOutput;
        use ruint::aliases::U256;

        // other outputs to be opened on the settlement layer/aggregation program
        let block_output = BlocksOutput {
            chain_id: U256::try_from(metadata.block_level_metadata.chain_id).unwrap(),
            first_block_timestamp: metadata.block_level_metadata.timestamp,
            last_block_timestamp: metadata.block_level_metadata.timestamp,
            pubdata_hash: pubdata_hash.into(),
            priority_ops_hashes_hash: self.l1_to_l2_txs_hash,
            l2_to_l1_logs_hashes_hash: l2_to_l1_logs_hashes_hash.into(),
            upgrade_tx_hash: self.upgrade_tx_hash,
        };

        use crate::system_implementation::system::BlocksPublicInput;
        let public_input = BlocksPublicInput {
            state_before: chain_state_commitment_before.hash().into(),
            state_after: chain_state_commitment_after.hash().into(),
            blocks_output: block_output.hash().into(),
        };

        (io_final_data, public_input.hash().into())
    }

    // fn finish(
    //     mut self,
    //     block_metadata: BlockMetadataFromOracle,
    //     current_block_hash: Bytes32,
    //     l1_to_l2_txs_hash: Bytes32,
    //     upgrade_tx_hash: Bytes32,
    //     result_keeper: &mut impl IOResultKeeper<EthereumIOTypesConfig>,
    //     mut logger: impl Logger,
    // ) -> Self::FinalData {
    //     let mut state_commitment = {
    //         use zk_ee::system_io_oracle::INITIALIZE_IO_IMPLEMENTER_QUERY_ID;

    //         // TODO (EVM-989): read only state commitment
    //         let fsm_state: BasicIOImplementerFSM<FlatStorageCommitment<TREE_HEIGHT>> = self
    //             .oracle
    //             .query_with_empty_input(INITIALIZE_IO_IMPLEMENTER_QUERY_ID)
    //             .unwrap();

    //         fsm_state.state_root_view
    //     };

    //     let mut blocks_hasher = Blake2s256::new();
    //     for block_hash in block_metadata.block_hashes.0.iter() {
    //         blocks_hasher.update(&block_hash.to_be_bytes::<32>());
    //     }

    //     // chain state before
    //     let chain_state_commitment_before = ChainStateCommitment {
    //         state_root: state_commitment.root,
    //         next_free_slot: state_commitment.next_free_slot,
    //         block_number: block_metadata.block_number - 1,
    //         last_256_block_hashes_blake: blocks_hasher.finalize().into(),
    //         // TODO(EVM-1080): we should set and validate that current block timestamp >= previous
    //         last_block_timestamp: 0,
    //     };

    //     // finishing IO, applying changes
    //     let mut pubdata_hasher = Blake2s256::new();
    //     pubdata_hasher.update(current_block_hash.as_u8_ref());
    //     let mut l2_to_l1_logs_hasher = Blake2s256::new();

    //     self.storage
    //         .finish(
    //             &mut self.oracle,
    //             Some(&mut state_commitment),
    //             &mut pubdata_hasher,
    //             result_keeper,
    //             &mut logger,
    //         )
    //         .expect("Failed to finish storage");
    //     self.logs_storage
    //         .apply_l2_to_l1_logs_hashes_to_hasher(&mut l2_to_l1_logs_hasher);
    //     self.logs_storage
    //         .apply_pubdata(&mut pubdata_hasher, result_keeper);
    //     result_keeper.logs(self.logs_storage.messages_ref_iter());
    //     result_keeper.events(self.events_storage.events_ref_iter());

    //     let pubdata_hash = pubdata_hasher.finalize();
    //     let l2_to_l1_logs_hashes_hash = l2_to_l1_logs_hasher.finalize();

    //     blocks_hasher = Blake2s256::new();
    //     for block_hash in block_metadata.block_hashes.0.iter().skip(1) {
    //         blocks_hasher.update(&block_hash.to_be_bytes::<32>());
    //     }
    //     blocks_hasher.update(current_block_hash.as_u8_ref());

    //     // chain state after
    //     let chain_state_commitment_after = ChainStateCommitment {
    //         state_root: state_commitment.root,
    //         next_free_slot: state_commitment.next_free_slot,
    //         block_number: block_metadata.block_number,
    //         last_256_block_hashes_blake: blocks_hasher.finalize().into(),
    //         // TODO(EVM-1080): we should set and validate that current block timestamp >= previous
    //         last_block_timestamp: 0,
    //     };

    //     // other outputs to be opened on the settlement layer/aggregation program
    //     let block_output = BlocksOutput {
    //         chain_id: U256::try_from(block_metadata.chain_id).unwrap(),
    //         first_block_timestamp: block_metadata.timestamp,
    //         last_block_timestamp: block_metadata.timestamp,
    //         pubdata_hash: pubdata_hash.into(),
    //         priority_ops_hashes_hash: l1_to_l2_txs_hash,
    //         l2_to_l1_logs_hashes_hash: l2_to_l1_logs_hashes_hash.into(),
    //         upgrade_tx_hash,
    //     };

    //     let public_input = BlocksPublicInput {
    //         state_before: chain_state_commitment_before.hash().into(),
    //         state_after: chain_state_commitment_after.hash().into(),
    //         blocks_output: block_output.hash().into(),
    //     };

    //     (self.oracle, public_input.hash().into())
    // }
}

// #[cfg(feature = "wrap-in-batch")]
// impl<
//         A: Allocator + Clone + Default,
//         R: Resources,
//         P: StorageAccessPolicy<R, Bytes32> + Default,
//         SC: StackCtor<N>,
//         const N: usize,
//         O: IOOracle,
//     > FinishIO for FullIO<A, R, P, SC, N, O, true>
// {
//     type FinalData = (O, Bytes32);
//     fn finish(
//         mut self,
//         block_metadata: BlockMetadataFromOracle,
//         current_block_hash: Bytes32,
//         _l1_to_l2_txs_hash: Bytes32,
//         upgrade_tx_hash: Bytes32,
//         result_keeper: &mut impl IOResultKeeper<EthereumIOTypesConfig>,
//         mut logger: impl Logger,
//     ) -> Self::FinalData {
//         let mut state_commitment = {
//             // TODO (EVM-989): read only state commitment
//             use zk_ee::system_io_oracle::INITIALIZE_IO_IMPLEMENTER_QUERY_ID;
//             let fsm_state: BasicIOImplementerFSM<FlatStorageCommitment<TREE_HEIGHT>> = self
//                 .oracle
//                 .query_with_empty_input(INITIALIZE_IO_IMPLEMENTER_QUERY_ID)
//                 .unwrap();

//             fsm_state.state_root_view
//         };

//         // chain state before
//         // currently we generate simplified commitment(only to state) for tests.
//         let _ = logger.write_fmt(format_args!(
//             "PI calculation: state commitment before {:?}\n",
//             state_commitment
//         ));
//         let mut chain_state_hasher = Blake2s256::new();
//         chain_state_hasher.update(state_commitment.root.as_u8_ref());
//         chain_state_hasher.update(state_commitment.next_free_slot.to_be_bytes());
//         let chain_state_commitment_before = chain_state_hasher.finalize();

//         // finishing IO, applying changes
//         let mut pubdata_hasher = crypto::sha3::Keccak256::new();
//         pubdata_hasher.update(current_block_hash.as_u8_ref());

//         self.storage
//             .finish(
//                 &mut self.oracle,
//                 Some(&mut state_commitment),
//                 &mut pubdata_hasher,
//                 result_keeper,
//                 &mut logger,
//             )
//             .expect("Failed to finish storage");

//         self.logs_storage
//             .apply_pubdata(&mut pubdata_hasher, result_keeper);
//         result_keeper.logs(self.logs_storage.messages_ref_iter());
//         result_keeper.events(self.events_storage.events_ref_iter());
//         let mut full_root_hasher = crypto::sha3::Keccak256::new();
//         full_root_hasher.update(self.logs_storage.tree_root().as_u8_ref());
//         full_root_hasher.update([0u8; 32]); // aggregated root 0 for now
//         let full_l2_to_l1_logs_root = full_root_hasher.finalize();
//         let l1_txs_commitment = self.logs_storage.l1_txs_commitment();

//         let pubdata_hash = pubdata_hasher.finalize();

//         // chain state after
//         // currently we generate simplified commitment(only to state) for tests.
//         let _ = logger.write_fmt(format_args!(
//             "PI calculation: state commitment after {:?}\n",
//             state_commitment
//         ));
//         let mut chain_state_hasher = Blake2s256::new();
//         chain_state_hasher.update(state_commitment.root.as_u8_ref());
//         chain_state_hasher.update(state_commitment.next_free_slot.to_be_bytes());
//         let chain_state_commitment_after = chain_state_hasher.finalize();

//         let mut da_commitment_hasher = crypto::sha3::Keccak256::new();
//         da_commitment_hasher.update([0u8; 32]); // we don't have to validate state diffs hash
//         da_commitment_hasher.update(pubdata_hash); // full pubdata keccak
//         da_commitment_hasher.update([1u8]); // with calldata we should provide 1 blob
//         da_commitment_hasher.update([0u8; 32]); // its hash will be ignored on the settlement layer
//         let da_commitment = da_commitment_hasher.finalize();
//         let batch_output = public_input::BatchOutput {
//             chain_id: U256::try_from(block_metadata.chain_id).unwrap(),
//             first_block_timestamp: block_metadata.timestamp,
//             last_block_timestamp: block_metadata.timestamp,
//             used_l2_da_validator_address: ruint::aliases::B160::ZERO,
//             pubdata_commitment: da_commitment.into(),
//             number_of_layer_1_txs: U256::try_from(l1_txs_commitment.0).unwrap(),
//             priority_operations_hash: l1_txs_commitment.1,
//             l2_logs_tree_root: full_l2_to_l1_logs_root.into(),
//             upgrade_tx_hash,
//         };
//         let _ = logger.write_fmt(format_args!(
//             "PI calculation: batch output {:?}\n",
//             batch_output,
//         ));

//         let public_input = public_input::BatchPublicInput {
//             state_before: chain_state_commitment_before.into(),
//             state_after: chain_state_commitment_after.into(),
//             batch_output: batch_output.hash().into(),
//         };
//         let _ = logger.write_fmt(format_args!(
//             "PI calculation: final batch public input {:?}\n",
//             public_input,
//         ));
//         let public_input_hash = public_input.hash().into();
//         let _ = logger.write_fmt(format_args!(
//             "PI calculation: final batch public input hash {:?}\n",
//             public_input_hash,
//         ));

//         (self.oracle, public_input_hash)
//     }
// }

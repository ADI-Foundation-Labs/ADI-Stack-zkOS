#![feature(allocator_api)]

use std::{path::PathBuf, str::FromStr};

use forward_system::run::{
    test_impl::{InMemoryPreimageSource, InMemoryTree, TxListSource},
    BatchContext, StorageCommitment,
};
use oracle_provider::{ReadWitnessSource, ZkEENonDeterminismSource};
pub mod helpers;

/// Runs the batch, and returns the output (that contains gas usage, transaction status etc.).
pub use forward_system::run::run_batch;
use zk_ee::common_structs::ProofData;

/// Runs a batch in riscV - using zksync_os binary - and returns the
/// witness that can be passed to the prover subsystem.
pub fn run_batch_generate_witness(
    block_context: BlockContext,
    tree: InMemoryTree,
    preimage_source: InMemoryPreimageSource,
    tx_source: TxListSource,
    proof_data: ProofData<StorageCommitment>,
    zksync_os_bin_path: &str,
) -> Vec<u32> {
    use forward_system::run::*;

    let block_metadata_reponsder = BlockMetadataResponder {
        block_metadata: batch_context,
    };
    let tx_data_reponder = TxDataResponder {
        tx_source,
        next_tx: None,
    };
    let preimage_responder = GenericPreimageResponder { preimage_source };
    let tree_responder = ReadTreeResponder { tree };
    let io_implementer_init_responder = IOImplementerInitResponder {
        io_implementer_init_data: Some(io_implementer_init_data(Some(storage_commitment))),
    };

    let mut oracle = ZkEENonDeterminismSource::default();
    oracle.add_external_processor(block_metadata_reponsder);
    oracle.add_external_processor(tx_data_reponder);
    oracle.add_external_processor(preimage_responder);
    oracle.add_external_processor(tree_responder);
    oracle.add_external_processor(io_implementer_init_responder);

    // We'll wrap the source, to collect all the reads.
    let copy_source = ReadWitnessSource::new(oracle);

    let items = copy_source.get_read_items();
    // By default - enable diagnostics is false (which makes the test run faster).
    let path = PathBuf::from_str(zksync_os_bin_path).unwrap();
    let output = zksync_os_runner::run(path, None, 1 << 36, copy_source);

    // We return 0s in case of failure.
    assert_ne!(output, [0u32; 8]);

    let result = items.borrow().clone();
    result
}

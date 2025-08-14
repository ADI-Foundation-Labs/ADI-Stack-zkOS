#![feature(allocator_api)]

use std::{path::PathBuf, str::FromStr};

use forward_system::run::{
    test_impl::{InMemoryPreimageSource, InMemoryTree, TxListSource},
    BatchContext, StorageCommitment,
};
use oracle_provider::ReadWitnessSource;
pub mod helpers;

/// Runs the batch, and returns the output (that contains gas usage, transaction status etc.).
use zk_ee::common_structs::ProofData;

/// Runs a block in riscV - using zksync_os binary - and returns the
/// witness that can be passed to the prover subsystem.
pub fn run_batch_generate_witness(
    batch_context: BatchContext,
    tree: InMemoryTree,
    preimage_source: InMemoryPreimageSource,
    tx_source: TxListSource,
    proof_data: ProofData<StorageCommitment>,
    zksync_os_bin_path: &str,
) -> Vec<u32> {
    use forward_system::run::*;

    let oracle = make_oracle_for_proofs_and_dumps_for_init_data(
        batch_context,
        tree,
        preimage_source,
        tx_source,
        Some(proof_data),
        false,
    );

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

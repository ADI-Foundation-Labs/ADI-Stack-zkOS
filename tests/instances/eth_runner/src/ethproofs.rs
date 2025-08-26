use crate::block::Block;
use crate::block_hashes::BlockHashes;
use crate::calltrace::CallTrace;
use crate::dump_utils::AccountStateDiffs;
use crate::live_run::rpc;
use crate::native_model::compute_ratio;
use crate::post_check::{post_check, post_check_ext};
use crate::prestate::{populate_prestate, DiffTrace, PrestateTrace};
use crate::receipts::{BlockReceipts, TransactionReceipt};
use alloy::consensus::Header;
use alloy::eips::eip4844::BlobTransactionSidecarItem;
use alloy_primitives::Address;
use alloy_primitives::U256;
use alloy_rlp::Encodable;
use alloy_rpc_types_eth::Withdrawal;
use anyhow::Context;
use anyhow::Ok;
use anyhow::Result;
use forward_system::run::output::map_tx_results;
use rig::log::info;
use rig::*;
use std::fs::{self, File};
use std::io::BufReader;

const ETH_CHAIN_ID: u64 = 1;

#[allow(clippy::too_many_arguments)]
fn eth_run(
    mut chain: Chain<false>,
    header: Header,
    block_number: u64,
    transactions: Vec<Vec<u8>>,
    block_hashes: Vec<U256>,
    witness: alloy_rpc_types_debug::ExecutionWitness,
    withdrawals_encoding: Vec<u8>,
    blobs: Vec<BlobTransactionSidecarItem>,
) -> anyhow::Result<()> {
    chain.set_last_block_number(block_number - 1);

    chain.set_block_hashes(block_hashes.try_into().unwrap());

    let witness_output_dir = {
        let mut suffix = block_number.to_string();
        suffix.push_str("_witness");
        std::path::PathBuf::from(&suffix)
    };
    let _result_keeper = chain.run_eth_block::<true>(
        transactions,
        witness,
        header,
        withdrawals_encoding,
        blobs,
        Some(witness_output_dir),
        None,
    );

    Ok(())
}

pub fn ethproofs_run(
    block_number: u64,
    reth_endpoint: &str,
    beacon_chain_endpoint: &str,
) -> anyhow::Result<()> {
    // Fetch data from RPC endpoints
    let block = rpc::get_block(reth_endpoint, block_number)
        .context(format!("Failed to fetch block for {block_number}"))?;
    let blobs = rpc::get_blobs_from_beacon_chain(beacon_chain_endpoint, &block.result.header)
        .context(format!("Failed to fetch blobs for {block_number}"))?;
    // Remove?
    let prestate = rpc::get_prestate(reth_endpoint, block_number)
        .context(format!("Failed to fetch prestate trace for {block_number}"))?;
    let witness = rpc::get_witness(reth_endpoint, block_number)
        .context(format!("Failed to fetch witness for {block_number}"))?
        .result;
    let block_hashes = rpc::fetch_block_hashes_array(reth_endpoint, block_number)
        .context(format!("Failed to fetch block hashes for {block_number}"))?
        .to_vec();

    info!("Running block: {block_number}");
    info!("Block gas used: {}", block.result.header.gas_used);
    let miner = block.result.header.beneficiary;
    let blobs: Vec<BlobTransactionSidecarItem> = blobs
        .into_iter()
        .enumerate()
        .map(|(idx, el)| BlobTransactionSidecarItem {
            index: idx as u64,
            blob: Box::default(),
            kzg_commitment: el.kzg_commitment,
            kzg_proof: el.kzg_proof,
        })
        .collect();

    let header = block.result.header.clone().into();
    let withdrawals = block
        .result
        .withdrawals
        .clone()
        .map(|el| el.0)
        .unwrap_or_default();
    let withdrawals_encoding = if let Some(withdrawals) = block.result.withdrawals.clone() {
        let mut buff = vec![];
        withdrawals.encode(&mut buff);

        buff
    } else {
        Vec::new()
    };
    let transactions = block.get_all_raw_transactions();

    let chain = Chain::empty(Some(ETH_CHAIN_ID));
    eth_run(
        chain,
        header,
        block_number,
        transactions,
        block_hashes,
        witness,
        withdrawals_encoding,
        blobs,
    )
}

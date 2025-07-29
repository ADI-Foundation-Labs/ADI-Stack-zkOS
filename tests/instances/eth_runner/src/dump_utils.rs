use crate::live_run::rpc;
use anyhow::Context;
use anyhow::Result;

pub fn dump_eth_block(block_number: u64, endpoint: &str, block_dir: String) -> Result<()> {
    let block = rpc::get_block(endpoint, block_number)
        .context(format!("Failed to fetch block for {block_number}"))?;
    let prestate = rpc::get_prestate(endpoint, block_number)
        .context(format!("Failed to fetch prestate trace for {block_number}"))?;
    let diff = rpc::get_difftrace(endpoint, block_number)
        .context(format!("Failed to fetch diff trace for {block_number}"))?;
    let receipts = rpc::get_receipts(endpoint, block_number)
        .context(format!("Failed to fetch block receipts for {block_number}"))?;
    let call = rpc::get_calltrace(endpoint, block_number)
        .context(format!("Failed to fetch call trace for {block_number}"))?;
    let witness = rpc::get_witness(endpoint, block_number)
        .context(format!("Failed to fetch witness for {block_number}"))?;
    let block_hashes = rpc::fetch_block_hashes_array(endpoint, block_number)
        .context(format!("Failed to fetch block hashes for {block_number}"))?
        .to_vec();

    use std::fs;
    use std::path::Path;

    let dir = Path::new(&block_dir);
    serde_json::to_writer(fs::File::create(dir.join("block.json"))?, &block)?;
    serde_json::to_writer(fs::File::create(dir.join("calltrace.json"))?, &call)?;
    serde_json::to_writer(fs::File::create(dir.join("receipts.json"))?, &receipts)?;
    serde_json::to_writer(fs::File::create(dir.join("prestatetrace.json"))?, &prestate)?;
    serde_json::to_writer(fs::File::create(dir.join("difftrace.json"))?, &diff)?;
    serde_json::to_writer(fs::File::create(dir.join("witness.json"))?, &witness)?;
    serde_json::to_writer(
        fs::File::create(dir.join("block_hashes.json"))?,
        &block_hashes,
    )?;

    Ok(())
}

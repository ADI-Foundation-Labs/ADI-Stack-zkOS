//!
//! These tests are focused on interop support in ZKsync OS
//!
#![cfg(test)]

use alloy::consensus::TxLegacy;
use alloy::primitives::TxKind;
use alloy::signers::local::PrivateKeySigner;
use arrayvec::ArrayVec;
use rig::alloy::primitives::address;
use rig::ruint::aliases::{B160, U256};
use rig::utils::{ERC_20_BYTECODE, ERC_20_MINT_CALLDATA};
use rig::zk_ee::system::metadata::InteropRoot;
use rig::zk_ee::utils::Bytes32;
use rig::{alloy, zksync_web3_rs, BlockContext, Chain};
use std::str::FromStr;
use zksync_web3_rs::signers::{LocalWallet, Signer};

#[test]
fn run_one_interop_root() {
    let mut chain = Chain::empty(None);
    let wallet = PrivateKeySigner::from_str(
        "dcf2cbdd171a21c480aa7f53d77f31bb102282b3ff099c78e3118b37348c72f7",
    )
    .unwrap();
    let wallet_ethers = LocalWallet::from_bytes(wallet.to_bytes().as_slice()).unwrap();

    let from = wallet_ethers.address();
    let to = address!("0000000000000000000000000000000000010002");

    let bytecode = hex::decode(ERC_20_BYTECODE).unwrap();
    chain.set_evm_bytecode(B160::from_be_bytes(to.into_array()), &bytecode);

    chain.set_balance(
        B160::from_be_bytes(from.0),
        U256::from(1_000_000_000_000_000_u64),
    );

    let encoded_mint_tx = {
        let mint_tx = TxLegacy {
            chain_id: 37u64.into(),
            nonce: 0,
            gas_price: 1000,
            gas_limit: 80_000,
            to: TxKind::Call(to),
            value: Default::default(),
            input: hex::decode(ERC_20_MINT_CALLDATA).unwrap().into(),
        };
        rig::utils::sign_and_encode_alloy_tx(mint_tx, &wallet)
    };

    let mut block_context = BlockContext::default();

    let mut interops_roots = ArrayVec::new();

    interops_roots.push(InteropRoot {
        root: Bytes32::ZERO,
        block_or_batch_number: 42,
        chain_id: 1,
    });

    block_context.interop_roots = interops_roots.into();

    chain.run_block_with_extra_stats(vec![encoded_mint_tx], Some(block_context), None, None, None);
}

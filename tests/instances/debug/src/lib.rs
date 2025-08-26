//!
//! These tests are focused on different tx types, AA features.
//!
#![cfg(test)]

use std::path::PathBuf;
use alloy::consensus::{TxEip1559, TxEip2930, TxLegacy};
use alloy::primitives::TxKind;
use alloy::signers::local::PrivateKeySigner;
use hex::FromHex;
use rig::alloy::consensus::TxEip7702;
use rig::alloy::primitives::{address, FixedBytes};
use rig::alloy::rpc::types::{AccessList, AccessListItem, TransactionRequest};
use rig::ethers::types::Address;
use rig::ruint::aliases::{B160, U256};
use rig::utils::*;
use rig::{alloy, ethers, zksync_web3_rs, Chain};
use std::str::FromStr;
use zksync_web3_rs::eip712::Eip712Meta;
use zksync_web3_rs::eip712::PaymasterParams;
use zksync_web3_rs::signers::{LocalWallet, Signer};

fn run_base_system_common(use_aa: bool, use_paymaster: bool) {
    let mut chain = Chain::empty(None);

    chain.run_block(vec![], None, None);
}

#[test]
fn run_base_system() {
    run_base_system_common(false, false);
}

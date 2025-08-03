use crypto::MiniDigest;
use zk_ee::utils::Bytes32;

pub trait BlockDataKeeper: core::fmt::Debug {
    fn new() -> Self;

    fn start_transaction(&mut self);
    fn record_transaction_hash(&mut self, tx_hash: &Bytes32);
    fn record_enforced_transaction_hash(&mut self, tx_hash: &Bytes32);
    fn record_upgrade_transaction_hash(&mut self, tx_hash: &Bytes32);
    fn record_gas_used_by_transaction(&mut self, gas: u64);
    fn finish_transaction(&mut self);
}

#[derive(Debug)]
pub struct ZKBasicBlockDataKeeper {
    pub current_transaction_number: u32,
    pub transaction_hashes_accumulator: RollingKeccakHash,
    pub enforced_transaction_hashes_accumulator: AccumulatingBlake2sHash,
    pub upgrade_tx_recorder: UpgradeTx,
    pub block_gas_used: u64,
}

impl BlockDataKeeper for ZKBasicBlockDataKeeper {
    fn new() -> Self {
        Self {
            current_transaction_number: 0,
            transaction_hashes_accumulator: RollingKeccakHash {
                inner: Bytes32::ZERO,
                hasher: crypto::sha3::Keccak256::new(),
            },
            enforced_transaction_hashes_accumulator: AccumulatingBlake2sHash {
                hasher: crypto::blake2s::Blake2s256::new(),
            },
            upgrade_tx_recorder: UpgradeTx {
                inner: Bytes32::ZERO,
            },
            block_gas_used: 0,
        }
    }

    fn start_transaction(&mut self) {
        // nothing
    }

    fn record_transaction_hash(&mut self, tx_hash: &Bytes32) {
        self.transaction_hashes_accumulator.add_tx_hash(tx_hash);
    }

    fn record_enforced_transaction_hash(&mut self, tx_hash: &Bytes32) {
        self.enforced_transaction_hashes_accumulator
            .add_tx_hash(tx_hash);
    }

    fn record_upgrade_transaction_hash(&mut self, tx_hash: &Bytes32) {
        self.upgrade_tx_recorder.add_upgrade_tx_hash(tx_hash);
    }

    fn record_gas_used_by_transaction(&mut self, gas: u64) {
        self.block_gas_used += gas;
    }

    fn finish_transaction(&mut self) {
        self.current_transaction_number += 1;
    }
}

#[derive(Debug)]
pub struct RollingKeccakHash {
    inner: Bytes32,
    hasher: crypto::sha3::Keccak256,
}

impl RollingKeccakHash {
    fn add_tx_hash(&mut self, tx_hash: &Bytes32) {
        if self.inner.is_zero() {
            self.inner = *tx_hash;
        } else {
            self.inner = Bytes32::from_array({
                self.hasher.update(self.inner.as_u8_array_ref());
                self.hasher.update(tx_hash.as_u8_array_ref());
                self.hasher.finalize_reset()
            });
        }
    }

    pub fn finish(self) -> Bytes32 {
        self.inner
    }
}

#[derive(Debug)]
pub struct AccumulatingBlake2sHash {
    hasher: crypto::blake2s::Blake2s256,
}

impl AccumulatingBlake2sHash {
    fn add_tx_hash(&mut self, tx_hash: &Bytes32) {
        self.hasher.update(tx_hash.as_u8_array_ref());
    }

    pub fn finish(self) -> Bytes32 {
        Bytes32::from_array(self.hasher.finalize())
    }
}

#[derive(Debug)]
pub struct UpgradeTx {
    inner: Bytes32,
}

impl UpgradeTx {
    fn add_upgrade_tx_hash(&mut self, tx_hash: &Bytes32) {
        if self.inner.is_zero() == false {
            panic!("duplicate upgrade tx");
        }
        self.inner = *tx_hash;
    }

    pub fn finish(self) -> Bytes32 {
        self.inner
    }
}

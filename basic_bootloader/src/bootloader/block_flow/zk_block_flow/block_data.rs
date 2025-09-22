use crate::bootloader::block_flow::BlockTransactionsDataCollector;
use crate::bootloader::zk::ZkTransactionFlowOnlyEOA;
use crate::bootloader::BasicTransactionFlow;
use crate::bootloader::ExecutionResult;
use crypto::MiniDigest;
use ruint::aliases::B160;
use ruint::aliases::U256;
use zk_ee::metadata_markers::basic_metadata::BasicMetadata;
use zk_ee::metadata_markers::basic_metadata::ZkSpecificPricingMetadata;
use zk_ee::system::*;
use zk_ee::utils::Bytes32;

///
/// Transaction data keeper used to collect information during tx execution to later be used during post-op.
/// It has generic enforced(l1) txs hashes accumulator, as it needs to be different for different post-ops(sequencing, proving aggregation, proving batch, etc).
///
#[derive(Debug)]
pub struct ZKBasicTransactionDataKeeper<EA: EnforcedTxHashesAccumulator> {
    pub current_transaction_number: u32,
    pub transaction_hashes_accumulator: RollingKeccakHashWithCount,
    pub enforced_transaction_hashes_accumulator: EA,
    pub upgrade_tx_recorder: UpgradeTx,
    pub block_gas_used: u64,
    pub block_pubdata_used: u64,
    pub block_computational_native_used: u64,
}

impl<EA: EnforcedTxHashesAccumulator> ZKBasicTransactionDataKeeper<EA> {
    pub fn new() -> Self {
        Self {
            current_transaction_number: 0,
            transaction_hashes_accumulator: RollingKeccakHashWithCount::empty(),
            enforced_transaction_hashes_accumulator: EA::empty(),
            upgrade_tx_recorder: UpgradeTx {
                inner: Bytes32::ZERO,
            },
            block_gas_used: 0,
            block_pubdata_used: 0,
            block_computational_native_used: 0,
        }
    }
}

impl<S: EthereumLikeTypes, EA: EnforcedTxHashesAccumulator>
    BlockTransactionsDataCollector<S, ZkTransactionFlowOnlyEOA<S>>
    for ZKBasicTransactionDataKeeper<EA>
where
    S::IO: IOSubsystemExt,
    S::Metadata: ZkSpecificPricingMetadata,
    <S::Metadata as BasicMetadata<S::IOTypes>>::TransactionMetadata: From<(B160, U256)>,
{
    fn record_transaction_results(
        &mut self,
        _system: &System<S>,
        _transaction: <ZkTransactionFlowOnlyEOA<S> as BasicTransactionFlow<S>>::Transaction<'_>,
        _context: &<ZkTransactionFlowOnlyEOA<S> as BasicTransactionFlow<S>>::TransactionContext,
        _result: &ExecutionResult<'_, <S as SystemTypes>::IOTypes>,
    ) {
        // here we are not interested in data as of yet
    }
}

pub trait EnforcedTxHashesAccumulator: core::fmt::Debug {
    fn empty() -> Self;
    fn add_tx_hash(&mut self, tx_hash: &Bytes32);

    fn finish(self) -> Bytes32;
}

#[derive(Debug)]
pub struct NopTxHashesAccumulator;

impl EnforcedTxHashesAccumulator for NopTxHashesAccumulator {
    fn empty() -> Self {
        Self
    }

    fn add_tx_hash(&mut self, _tx_hash: &Bytes32) {}

    fn finish(self) -> Bytes32 {
        Bytes32::ZERO
    }
}

impl EnforcedTxHashesAccumulator for () {
    fn empty() -> Self {
        ()
    }

    fn add_tx_hash(&mut self, _tx_hash: &Bytes32) {}

    fn finish(self) -> Bytes32 {
        Bytes32::ZERO
    }
}

#[derive(Debug)]
pub struct RollingKeccakHashWithCount {
    inner: Bytes32,
    hasher: crypto::sha3::Keccak256,
    pub count: u32,
}

impl EnforcedTxHashesAccumulator for RollingKeccakHashWithCount {
    fn empty() -> Self {
        Self {
            inner: Bytes32::ZERO,
            hasher: crypto::sha3::Keccak256::new(),
            count: 0,
        }
    }

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

    fn finish(self) -> Bytes32 {
        self.inner
    }
}

#[derive(Debug)]
pub struct AccumulatingBlake2sHash {
    hasher: crypto::blake2s::Blake2s256,
}

impl EnforcedTxHashesAccumulator for AccumulatingBlake2sHash {
    fn empty() -> Self {
        Self {
            hasher: crypto::blake2s::Blake2s256::new(),
        }
    }

    fn add_tx_hash(&mut self, tx_hash: &Bytes32) {
        self.hasher.update(tx_hash.as_u8_array_ref());
    }

    fn finish(self) -> Bytes32 {
        Bytes32::from_array(self.hasher.finalize())
    }
}

#[derive(Debug)]
pub struct UpgradeTx {
    inner: Bytes32,
}

impl UpgradeTx {
    pub fn add_upgrade_tx_hash(&mut self, tx_hash: &Bytes32) {
        if self.inner.is_zero() == false {
            panic!("duplicate upgrade tx");
        }
        self.inner = *tx_hash;
    }

    pub fn finish(self) -> Bytes32 {
        self.inner
    }
}

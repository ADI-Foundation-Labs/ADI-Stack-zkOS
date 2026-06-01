use crate::system_implementation::system::da_commitment_generator::DACommitmentGenerator;
use crypto::sha3::Keccak256;
use crypto::MiniDigest;
use zk_ee::oracle::IOOracle;
use zk_ee::utils::write_bytes::WriteBytes;
use zk_ee::utils::Bytes32;

/// DA commitment for custom DA modes:
/// keccak256(state_diff_hash || keccak256(pubdata)).
///
/// By default, `state_diff_hash` is zero. It can be updated by caller via
/// [`DACommitmentGenerator::set_state_diff_hash`].
pub struct PubdataKeccak256CommitmentGenerator {
    state_diff_hash: Bytes32,
    pubdata_hasher: Keccak256,
}

impl PubdataKeccak256CommitmentGenerator {
    pub fn new() -> Self {
        Self {
            state_diff_hash: Bytes32::zero(),
            pubdata_hasher: Keccak256::new(),
        }
    }
}

impl WriteBytes for PubdataKeccak256CommitmentGenerator {
    fn write(&mut self, buf: &[u8]) {
        self.pubdata_hasher.update(buf);
    }
}

impl<O: IOOracle> DACommitmentGenerator<O> for PubdataKeccak256CommitmentGenerator {
    fn finalize(&mut self, _oracle: &mut O) -> Bytes32 {
        let pubdata_hash = self.pubdata_hasher.finalize_reset();

        let mut da_commitment_hasher = Keccak256::new();
        da_commitment_hasher.update(self.state_diff_hash.as_u8_ref());
        da_commitment_hasher.update(pubdata_hash);
        da_commitment_hasher.finalize().into()
    }

    fn set_state_diff_hash(&mut self, state_diff_hash: Bytes32) {
        self.state_diff_hash = state_diff_hash;
    }
}


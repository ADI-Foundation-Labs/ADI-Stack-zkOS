use super::DACommitmentGenerator;
use crypto::sha3::Keccak256;
use crypto::MiniDigest;
use zk_ee::oracle::IOOracle;
use zk_ee::utils::write_bytes::WriteBytes;
use zk_ee::utils::Bytes32;

/// DA commitment for custom DA modes:
/// `keccak256(state_diff_hash || keccak256(pubdata))`.
///
/// By default, `state_diff_hash` is zero, as state diffs are not a part of the public input.
/// It can be updated by the caller via [`DACommitmentGenerator::set_state_diff_hash`].
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

impl Default for PubdataKeccak256CommitmentGenerator {
    fn default() -> Self {
        Self::new()
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
        da_commitment_hasher.update(pubdata_hash); // full pubdata keccak
        da_commitment_hasher.finalize().into()
    }

    fn set_state_diff_hash(&mut self, state_diff_hash: Bytes32) {
        self.state_diff_hash = state_diff_hash;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zk_ee::oracle::usize_serialization::{UsizeDeserializable, UsizeSerializable};
    use zk_ee::system::errors::internal::InternalError;

    struct DummyOracle;

    impl IOOracle for DummyOracle {
        type RawIterator<'a> = core::iter::Empty<usize>;

        fn raw_query<'a, I: UsizeSerializable + UsizeDeserializable>(
            &'a mut self,
            _query_type: u32,
            _input: &I,
        ) -> Result<Self::RawIterator<'a>, InternalError> {
            Ok(core::iter::empty())
        }
    }

    const PUBDATA: &[u8] = b"some pubdata";

    fn expected_commitment(state_diff_hash: Bytes32) -> Bytes32 {
        let mut pubdata_hasher = Keccak256::new();
        pubdata_hasher.update(PUBDATA);

        let mut hasher = Keccak256::new();
        hasher.update(state_diff_hash.as_u8_ref());
        hasher.update(pubdata_hasher.finalize());
        hasher.finalize().into()
    }

    fn commitment_for(state_diff_hash: Option<Bytes32>) -> Bytes32 {
        let mut generator = PubdataKeccak256CommitmentGenerator::new();
        if let Some(state_diff_hash) = state_diff_hash {
            DACommitmentGenerator::<DummyOracle>::set_state_diff_hash(
                &mut generator,
                state_diff_hash,
            );
        }
        // Pubdata is consumed in chunks, the commitment must not depend on the chunking.
        let (head, tail) = PUBDATA.split_at(4);
        generator.write(head);
        generator.write(tail);

        generator.finalize(&mut DummyOracle)
    }

    #[test]
    fn state_diff_hash_is_zero_by_default() {
        assert_eq!(commitment_for(None), expected_commitment(Bytes32::ZERO));
    }

    #[test]
    fn state_diff_hash_is_committed_to_when_set() {
        let state_diff_hash = Bytes32::from_byte_fill(7);

        assert_eq!(
            commitment_for(Some(state_diff_hash)),
            expected_commitment(state_diff_hash)
        );
        assert_ne!(
            commitment_for(Some(state_diff_hash)),
            commitment_for(None),
            "state diff hash must affect the commitment"
        );
    }
}

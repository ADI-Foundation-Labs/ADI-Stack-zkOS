pub type Keccak256 = sha3::Keccak256;

use crate::MiniDigest;

impl MiniDigest for Keccak256 {
    type HashOutput = [u8; 32];

    #[inline(always)]
    fn new() -> Self {
        <Keccak256 as sha3::Digest>::new()
    }

    #[inline(always)]
    fn digest(input: impl AsRef<[u8]>) -> Self::HashOutput {
        <Keccak256 as sha3::Digest>::digest(input).into()
    }

    #[inline(always)]
    fn update(&mut self, input: impl AsRef<[u8]>) {
        <Keccak256 as sha3::Digest>::update(self, input);
    }

    #[inline(always)]
    fn finalize(self) -> Self::HashOutput {
        <Keccak256 as sha3::Digest>::finalize(self).into()
    }

    #[inline(always)]
    fn finalize_reset(&mut self) -> Self::HashOutput {
        <Keccak256 as sha3::Digest>::finalize_reset(self).into()
    }
}
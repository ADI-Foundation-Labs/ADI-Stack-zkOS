use crate::cost_constants::{POINT_EVALUATION_COST_ERGS, POINT_EVALUATION_NATIVE_COST};
use crypto::ark_ec::pairing::Pairing;
use crypto::ark_ec::AffineRepr;
use crypto::ark_ff::{Field, PrimeField};
use zk_ee::common_traits::TryExtend;
use zk_ee::interface_error;
use zk_ee::out_of_return_memory;
use zk_ee::system::errors::subsystem::SubsystemError;
use zk_ee::system::*;

///
/// Point evaluation system function implementation.
///
pub struct PointEvaluationImpl;

impl<R: Resources> SystemFunction<R, PointEvaluationErrors> for PointEvaluationImpl {
    /// Returns `OutOfGas` if not enough resources provided, resources may be not touched.
    ///
    /// Returns `InvalidInputSize` error if `input_len` != 192,
    /// `InvalidPoint` if commitment or proof point encoded incorrectly,
    /// `InvalidScalar` if `z` or `y` scalars encoded incorrectly,
    /// `InvalidVersionedHash` if versioned hash doesn't correspond to the commitment,
    /// `PairingMismatch` if kzg proof pairing check failed.
    fn execute<D: TryExtend<u8> + ?Sized, A: core::alloc::Allocator + Clone>(
        input: &[u8],
        output: &mut D,
        resources: &mut R,
        _allocator: A,
    ) -> Result<(), SubsystemError<PointEvaluationErrors>> {
        cycle_marker::wrap_with_resources!("point_evaluation", resources, {
            point_evaluation_as_system_function_inner(input, output, resources)
        })
    }
}

pub const TRUSTED_SETUP_TAU_G2_BYTES: [u8; 96] = const {
    let Ok(res) = const_hex::const_decode_to_array(
        b"b5bfd7dd8cdeb128843bc287230af38926187075cbfbefa81009a2ce615ac53d2914e5870cb452d2afaaab24f3499f72185cbfee53492714734429b7b38608e23926c911cceceac9a36851477ba4c60b087041de621000edc98edada20c1def2"
    ) else {
        panic!()
    };

    res
};

pub const POINT_EVAL_PRECOMPILE_SUCCESS_RESPONSE: [u8; 64] = const {
    // u256_be(4096) || u256_be(BLS12-381 Fr characteristic)
    let Ok(res) = const_hex::const_decode_to_array(
        b"000000000000000000000000000000000000000000000000000000000000100073eda753299d7d483339d80809a1d80553bda402fffe5bfeffffffff00000001"
    ) else {
        panic!()
    };

    res
};
pub const KZG_VERSIONED_HASH_VERSION_BYTE: u8 = 0x01;

// We do not need internal representation, just canonical scalar
fn parse_scalar(input: &[u8; 32]) -> Result<<crypto::bls12_381::Fr as PrimeField>::BigInt, ()> {
    // Arkworks has strange format for integer serialization, so we do manually
    let mut repr = [0u64; 4];
    for (dst, src) in repr.iter_mut().zip(input.as_rchunks::<8>().1.iter().rev()) {
        *dst = u64::from_be_bytes(*src);
    }
    let repr = crypto::BigInt::new(repr);
    if repr >= crypto::bls12_381::Fr::MODULUS {
        Err(())
    } else {
        Ok(repr)
    }
}

fn versioned_hash_for_kzg(data: &[u8]) -> [u8; 32] {
    use crypto::sha256::Digest;
    let mut hash: [u8; 32] = crypto::sha256::Sha256::digest(data).into();
    hash[0] = KZG_VERSIONED_HASH_VERSION_BYTE;

    hash
}

fn parse_g1_compressed(input: &[u8]) -> Result<crypto::bls12_381::G1Affine, ()> {
    // format coincides with one defined in ZCash/Arkworks
    use crypto::ark_serialize::CanonicalDeserialize;
    crypto::bls12_381::G1Affine::deserialize_compressed(input).map_err(|_| ())
}

fn point_evaluation_as_system_function_inner<D: ?Sized + TryExtend<u8>, R: Resources>(
    input: &[u8],
    dst: &mut D,
    resources: &mut R,
) -> Result<(), SubsystemError<PointEvaluationErrors>> {
    resources.charge(&R::from_ergs_and_native(
        POINT_EVALUATION_COST_ERGS,
        <R::Native as zk_ee::system::Computational>::from_computational(
            POINT_EVALUATION_NATIVE_COST,
        ),
    ))?;

    use crypto::ark_serialize::CanonicalDeserialize;
    let g2_by_tau_point = <crypto::bls12_381::curves::Bls12_381 as crypto::ark_ec::pairing::Pairing>::G2Affine::deserialize_compressed(&TRUSTED_SETUP_TAU_G2_BYTES[..]).expect("must decode from trusted setup");
    let prepared_g2_generator: <crypto::bls12_381::curves::Bls12_381 as crypto::ark_ec::pairing::Pairing>::G2Prepared = crypto::bls12_381::G2Affine::generator().into();

    if input.len() != 192 {
        return Err(PointEvaluationSubsystemError::LeafUsage(interface_error!(
            PointEvaluationInterfaceError::InvalidInputSize
        )));
    }

    // Each check without any parsing
    let versioned_hash = &input[..32];
    let commitment = &input[96..144];

    // so far it's just one version
    if versioned_hash_for_kzg(commitment) != versioned_hash {
        return Err(PointEvaluationSubsystemError::LeafUsage(interface_error!(
            PointEvaluationInterfaceError::InvalidVersionedHash
        )));
    }

    // Parse the commitment and proof
    let Ok(commitment_point) = parse_g1_compressed(commitment) else {
        return Err(PointEvaluationSubsystemError::LeafUsage(interface_error!(
            PointEvaluationInterfaceError::InvalidPoint
        )));
    };
    let proof = &input[144..192];
    let Ok(proof) = parse_g1_compressed(proof) else {
        return Err(PointEvaluationSubsystemError::LeafUsage(interface_error!(
            PointEvaluationInterfaceError::InvalidPoint
        )));
    };

    let Ok(z) = parse_scalar(input[32..64].try_into().unwrap()) else {
        return Err(PointEvaluationSubsystemError::LeafUsage(interface_error!(
            PointEvaluationInterfaceError::InvalidScalar
        )));
    };

    let Ok(y) = parse_scalar(input[64..96].try_into().unwrap()) else {
        return Err(PointEvaluationSubsystemError::LeafUsage(interface_error!(
            PointEvaluationInterfaceError::InvalidScalar
        )));
    };

    // e(y - P, G₂) * e(proof, X - z) == 1
    let mut y_minus_p = crypto::bls12_381::G1Affine::generator().mul_bigint(&y);
    y_minus_p -= &commitment_point;

    let mut g2_el: crypto::bls12_381::G2Projective = g2_by_tau_point.into();
    let z_in_g2 = crypto::bls12_381::G2Affine::generator().mul_bigint(&z);
    g2_el -= z_in_g2;

    use crypto::ark_ec::CurveGroup;
    let y_minus_p_prepared: crypto::bls12_381::G1Affine = y_minus_p.into_affine();
    let g2_el: <crypto::bls12_381::curves::Bls12_381 as crypto::ark_ec::pairing::Pairing>::G2Prepared = g2_el.into_affine().into();

    let gt_el = crypto::bls12_381::curves::Bls12_381::multi_pairing(
        [y_minus_p_prepared, proof],
        [prepared_g2_generator.clone(), g2_el],
    );
    if gt_el.0 == <crypto::bls12_381::curves::Bls12_381 as crypto::ark_ec::pairing::Pairing>::TargetField::ONE {
        dst.try_extend(POINT_EVAL_PRECOMPILE_SUCCESS_RESPONSE).map_err(|_| out_of_return_memory!())?;
        Ok(())
    } else {
        Err(PointEvaluationSubsystemError::LeafUsage(
            interface_error!(PointEvaluationInterfaceError::PairingMismatch),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use evm_interpreter::ERGS_PER_GAS;
    use zk_ee::system::Resource;
    use zk_ee::reference_implementations::BaseResources;
    use zk_ee::reference_implementations::DecreasingNative;
    use std::alloc::Global;

    use alloy_primitives::hex;

    type TestResources = BaseResources<DecreasingNative>;

    fn infinite_resources() -> TestResources {
        TestResources::FORMAL_INFINITE
    }

    #[test]
    fn basic_test() {
        // Test data from: https://github.com/ethereum/c-kzg-4844/blob/main/tests/verify_kzg_proof/kzg-mainnet/verify_kzg_proof_case_correct_proof_4_4/data.yaml

        let commitment = hex!("8f59a8d2a1a625a17f3fea0fe5eb8c896db3764f3185481bc22f91b4aaffcca25f26936857bc3a7c2539ea8ec3a952b7").to_vec();

        use crypto::sha256::*;
        let mut hasher = Sha256::new();
        hasher.update(commitment.clone());
        let mut versioned_hash  = hasher.finalize().to_vec();
        versioned_hash[0] = KZG_VERSIONED_HASH_VERSION_BYTE;

        let z = hex!("73eda753299d7d483339d80809a1d80553bda402fffe5bfeffffffff00000000").to_vec();
        let y = hex!("1522a4a7f34e1ea350ae07c29c96c7e79655aa926122e95fe69fcbd932ca49e9").to_vec();
        let proof = hex!("a62ad71d14c5719385c0686f1871430475bf3a00f0aa3f7b8dd99a9abc2160744faf0070725e00b60ad9a026a15b1a8c").to_vec();

        let input = [versioned_hash, z, y, commitment, proof].concat();

        let expected_output = hex!("000000000000000000000000000000000000000000000000000000000000100073eda753299d7d483339d80809a1d80553bda402fffe5bfeffffffff00000001");
        let gas = 50000;

        let mut output = Vec::new();
        let mut resources = infinite_resources();
        let gas_before = resources.ergs().0 / ERGS_PER_GAS;

        let result = PointEvaluationImpl::execute(&input, &mut output, &mut resources, Global);
        assert!(result.is_ok(), "Result: {:?}", result);

        let gas_used = gas_before - resources.ergs().0 / ERGS_PER_GAS;

        assert_eq!(gas_used, gas);
        assert_eq!(output[..], expected_output);
    }


    /// Test invalid input size - too short
    #[test]
    fn test_point_evaluation_invalid_input_size_short() {
        let input = vec![0u8; 191]; // One byte short
        let mut output = Vec::new();
        let mut resources = infinite_resources();

        let result = PointEvaluationImpl::execute(&input, &mut output, &mut resources, Global);

        assert!(result.is_err());
        if let Err(SubsystemError::LeafUsage(err)) = result {
            if let PointEvaluationInterfaceError::InvalidInputSize = err.0 {
                // Expected error
            } else {
                panic!("Expected InvalidInputSize error, got: {:?}", err);
            }
        } else {
            panic!("Expected InvalidInputSize error, got: {:?}", result);
        }
    }

    /// Test invalid input size - too long
    #[test]
    fn test_point_evaluation_invalid_input_size_long() {
        let input = vec![0u8; 193]; // One byte too long
        let mut output = Vec::new();
        let mut resources = infinite_resources();

        let result = PointEvaluationImpl::execute(&input, &mut output, &mut resources, Global);

        assert!(result.is_err());
        if let Err(SubsystemError::LeafUsage(err)) = result {
            if let PointEvaluationInterfaceError::InvalidInputSize = err.0 {
                // Expected error
            } else {
                panic!("Expected InvalidInputSize error, got: {:?}", err);
            }
        } else {
            panic!("Expected InvalidInputSize error, got: {:?}", result);
        }
    }

    /// Test invalid scalar - z >= field modulus
    #[test]
    fn test_point_evaluation_invalid_scalar_z() {
        let commitment = hex!("8f59a8d2a1a625a17f3fea0fe5eb8c896db3764f3185481bc22f91b4aaffcca25f26936857bc3a7c2539ea8ec3a952b7").to_vec();

        use crypto::sha256::*;
        let mut hasher = Sha256::new();
        hasher.update(commitment.clone());
        let mut versioned_hash  = hasher.finalize().to_vec();
        versioned_hash[0] = KZG_VERSIONED_HASH_VERSION_BYTE;

        // Set z to field modulus (invalid)
        let invalid_z = [
            0x73, 0xed, 0xa7, 0x53, 0x29, 0x9d, 0x7d, 0x48,
            0x33, 0x39, 0xd8, 0x08, 0x09, 0xa1, 0xd8, 0x05,
            0x53, 0xbd, 0xa4, 0x02, 0xff, 0xfe, 0x5b, 0xfe,
            0xff, 0xff, 0xff, 0xff, 0x00, 0x00, 0x00, 0x01
        ].to_vec();
        let y = hex!("1522a4a7f34e1ea350ae07c29c96c7e79655aa926122e95fe69fcbd932ca49e9").to_vec();
        let proof = hex!("a62ad71d14c5719385c0686f1871430475bf3a00f0aa3f7b8dd99a9abc2160744faf0070725e00b60ad9a026a15b1a8c").to_vec();

        let input = [versioned_hash, invalid_z, y, commitment, proof].concat();

        let mut output = Vec::new();
        let mut resources = infinite_resources();

        let result = PointEvaluationImpl::execute(&input, &mut output, &mut resources, Global);

        assert!(result.is_err());
        if let Err(SubsystemError::LeafUsage(err)) = result {
            if let PointEvaluationInterfaceError::InvalidScalar = err.0 {
                // Expected error
            } else {
                panic!("Expected InvalidScalar error, got: {:?}", err);
            }
        } else {
            panic!("Expected InvalidScalar error, got: {:?}", result);
        }
    }

    /// Test invalid scalar - y >= field modulus
    #[test]
    fn test_point_evaluation_invalid_scalar_y() {
        let commitment = hex!("8f59a8d2a1a625a17f3fea0fe5eb8c896db3764f3185481bc22f91b4aaffcca25f26936857bc3a7c2539ea8ec3a952b7").to_vec();

        use crypto::sha256::*;
        let mut hasher = Sha256::new();
        hasher.update(commitment.clone());
        let mut versioned_hash  = hasher.finalize().to_vec();
        versioned_hash[0] = KZG_VERSIONED_HASH_VERSION_BYTE;

        let z = hex!("73eda753299d7d483339d80809a1d80553bda402fffe5bfeffffffff00000000").to_vec();
        // Set y to field modulus (invalid)
        let invalid_y = [
            0x73, 0xed, 0xa7, 0x53, 0x29, 0x9d, 0x7d, 0x48,
            0x33, 0x39, 0xd8, 0x08, 0x09, 0xa1, 0xd8, 0x05,
            0x53, 0xbd, 0xa4, 0x02, 0xff, 0xfe, 0x5b, 0xfe,
            0xff, 0xff, 0xff, 0xff, 0x00, 0x00, 0x00, 0x01
        ].to_vec();
        let proof = hex!("a62ad71d14c5719385c0686f1871430475bf3a00f0aa3f7b8dd99a9abc2160744faf0070725e00b60ad9a026a15b1a8c").to_vec();

        let input = [versioned_hash, z, invalid_y, commitment, proof].concat();

        let mut output = Vec::new();
        let mut resources = infinite_resources();

        let result = PointEvaluationImpl::execute(&input, &mut output, &mut resources, Global);

        assert!(result.is_err());
        if let Err(SubsystemError::LeafUsage(err)) = result {
            if let PointEvaluationInterfaceError::InvalidScalar = err.0 {
                // Expected error
            } else {
                panic!("Expected InvalidScalar error, got: {:?}", err);
            }
        } else {
            panic!("Expected InvalidScalar error, got: {:?}", result);
        }
    }

    /// Test versioned hash computation function
    #[test]
    fn test_versioned_hash_for_kzg() {
        let commitment = [0u8; 48]; // Identity commitment
        let hash = versioned_hash_for_kzg(&commitment);

        // Should have correct version byte
        assert_eq!(hash[0], KZG_VERSIONED_HASH_VERSION_BYTE);

        // Should be deterministic
        let hash2 = versioned_hash_for_kzg(&commitment);
        assert_eq!(hash, hash2);
    }

    /// Test scalar parsing edge cases
    #[test]
    fn test_parse_scalar_edge_cases() {
        // Test maximum valid scalar (modulus - 1)
        let max_valid = [
            0x73, 0xed, 0xa7, 0x53, 0x29, 0x9d, 0x7d, 0x48,
            0x33, 0x39, 0xd8, 0x08, 0x09, 0xa1, 0xd8, 0x05,
            0x53, 0xbd, 0xa4, 0x02, 0xff, 0xfe, 0x5b, 0xfe,
            0xff, 0xff, 0xff, 0xff, 0x00, 0x00, 0x00, 0x00
        ];
        assert!(parse_scalar(&max_valid).is_ok());

        // Test minimum invalid scalar (modulus)
        let min_invalid = [
            0x73, 0xed, 0xa7, 0x53, 0x29, 0x9d, 0x7d, 0x48,
            0x33, 0x39, 0xd8, 0x08, 0x09, 0xa1, 0xd8, 0x05,
            0x53, 0xbd, 0xa4, 0x02, 0xff, 0xfe, 0x5b, 0xfe,
            0xff, 0xff, 0xff, 0xff, 0x00, 0x00, 0x00, 0x01
        ];
        assert!(parse_scalar(&min_invalid).is_err());

        // Test zero (always valid)
        let zero = [0u8; 32];
        assert!(parse_scalar(&zero).is_ok());
    }
}

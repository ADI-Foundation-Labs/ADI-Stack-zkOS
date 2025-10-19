use super::*;
use crate::cost_constants::{ECRECOVER_COST_ERGS, ECRECOVER_NATIVE_COST};
use zk_ee::system::base_system_functions::{Secp256k1ECRecoverErrors, SystemFunction};
use zk_ee::system::errors::{subsystem::SubsystemError, system::SystemError};
use zk_ee::system::{Computational, SystemFunctionExt};

///
/// ecrecover system function implementation.
///
pub struct EcRecoverImpl;

impl<R: Resources> SystemFunction<R, Secp256k1ECRecoverErrors> for EcRecoverImpl {
    /// If the input size is less than expected - it will be padded with zeroes.
    /// If the input size is greater - redundant bytes will be ignored.
    /// If the input is invalid(v != 27|28 or failed to recover signer) returns `Ok(0)`.
    ///
    /// Returns `OutOfGas` if not enough resources provided.
    fn execute<D: Extend<u8> + ?Sized, A: core::alloc::Allocator + Clone>(
        input: &[u8],
        output: &mut D,
        resources: &mut R,
        _allocator: A,
    ) -> Result<(), SubsystemError<Secp256k1ECRecoverErrors>> {
        Ok(cycle_marker::wrap_with_resources!(
            "ecrecover",
            resources,
            { ecrecover_as_system_function_inner(input, output, resources) }
        )?)
    }
}

impl<R: Resources> SystemFunctionExt<R, Secp256k1ECRecoverErrors> for EcRecoverImpl {
    /// If the input size is less than expected - it will be padded with zeroes.
    /// If the input size is greater - redundant bytes will be ignored.
    /// If the input is invalid(v != 27|28 or failed to recover signer) returns `Ok(0)`.
    ///
    /// Returns `OutOfGas` if not enough resources provided.
    fn execute<O: zk_ee::system_io_oracle::IOOracle, L: zk_ee::system::logger::Logger, D: Extend<u8> + ?Sized, A: core::alloc::Allocator + Clone>(
        input: &[u8],
        output: &mut D,
        resources: &mut R,
        oracle: &mut O,
        logger: &mut L,
        _allocator: A,
    ) -> Result<(), SubsystemError<Secp256k1ECRecoverErrors>> {
        #[cfg(not(target_arch = "riscv32"))]
        {
            Ok(cycle_marker::wrap_with_resources!(
                "ecrecover",
                resources,
                { ecrecover_as_system_function_inner(input, output, resources) }
            )?)
        }

        #[cfg(target_arch = "riscv32")]
        {
            #[cfg(not(feature = "ecrecover_with_oracle"))]
            const USE_ORACLE: bool = false;

            #[cfg(feature = "ecrecover_with_oracle")]
            const USE_ORACLE: bool = true;

            if USE_ORACLE {
                Ok(cycle_marker::wrap_with_resources!(
                    "ecrecover",
                    resources,
                    { ecrecover_as_system_function_inner_with_oracle(input, output, resources, oracle, logger) }
                )?)
            } else {
                Ok(cycle_marker::wrap_with_resources!(
                    "ecrecover",
                    resources,
                    { ecrecover_as_system_function_inner(input, output, resources) }
                )?)
            }
        }
    }
}

fn ecrecover_as_system_function_inner<
    S: ?Sized + MinimalByteAddressableSlice,
    D: ?Sized + Extend<u8>,
    R: Resources,
>(
    src: &S,
    dst: &mut D,
    resources: &mut R,
) -> Result<(), SystemError> {
    resources.charge(&R::from_ergs_and_native(
        ECRECOVER_COST_ERGS,
        R::Native::from_computational(ECRECOVER_NATIVE_COST),
    ))?;
    // digest, v, r, s in ABI
    let mut buffer = [0u8; 128];
    for (dst, src) in buffer.iter_mut().zip(src.iter()) {
        *dst = *src;
    }

    // follow https://github.com/ethereum/go-ethereum/blob/aadcb886753079d419f966a3bc990f708f8d1c3b/core/vm/contracts.go#L188

    let mut it = buffer.as_chunks::<32>().0.iter();
    let recovered_pubkey_bytes = unsafe {
        let digest = it.next().unwrap_unchecked();
        let v = it.next().unwrap_unchecked();
        let r = it.next().unwrap_unchecked();
        let s = it.next().unwrap_unchecked();

        if v[..31].iter().all(|el| *el == 0) == false {
            return Ok(());
        }

        let rec_id = v[31].wrapping_sub(27);
        if (rec_id == 0 || rec_id == 1) == false {
            return Ok(());
        }

        let Ok(pk_bytes) = ecrecover_inner(digest, r, s, rec_id) else {
            return Ok(());
        };

        pk_bytes
    };
    let bytes_ref = recovered_pubkey_bytes.as_ref();

    use crypto::sha3::Keccak256;
    use crypto::MiniDigest;
    let address_hash = Keccak256::digest(&bytes_ref[1..]);

    dst.extend(core::iter::repeat_n(0, 12).chain(address_hash.into_iter().skip(12)));

    Ok(())
}

pub fn ecrecover_inner(
    digest: &[u8; 32],
    r: &[u8; 32],
    s: &[u8; 32],
    rec_id: u8,
) -> Result<crypto::k256::EncodedPoint, ()> {
    use crypto::k256::{
        ecdsa::{hazmat::bits2field, RecoveryId, Signature},
        elliptic_curve::ops::Reduce,
        Scalar,
    };

    let signature = Signature::from_scalars(*r, *s).map_err(|_| ())?;
    let recovery_id = RecoveryId::try_from(rec_id).map_err(|_| ())?;

    let message = <Scalar as Reduce<crypto::k256::U256>>::reduce_bytes(
        &bits2field::<crypto::k256::Secp256k1>(digest).map_err(|_| ())?,
    );

    let Ok(pk) = crypto::secp256k1::recover(&message, &signature, &recovery_id) else {
        return Err(());
    };

    // represent as bytes, and we do not need compression
    let encoded = pk.to_encoded_point(false);

    Ok(encoded)
}

#[cfg(target_arch = "riscv32")]
fn ecrecover_as_system_function_inner_with_oracle<
    S: ?Sized + MinimalByteAddressableSlice,
    D: ?Sized + Extend<u8>,
    R: Resources,
    O: zk_ee::system_io_oracle::IOOracle,
    L: zk_ee::system::logger::Logger
>(
    src: &S,
    dst: &mut D,
    resources: &mut R,
    oracle: &mut O,
    logger: &mut L,
) -> Result<(), SystemError> {
    resources.charge(&R::from_ergs_and_native(
        ECRECOVER_COST_ERGS,
        R::Native::from_computational(ECRECOVER_NATIVE_COST),
    ))?;
    // digest, v, r, s in ABI
    let mut buffer = [0u8; 128];
    for (dst, src) in buffer.iter_mut().zip(src.iter()) {
        *dst = *src;
    }

    // follow https://github.com/ethereum/go-ethereum/blob/aadcb886753079d419f966a3bc990f708f8d1c3b/core/vm/contracts.go#L188

    let mut it = buffer.as_chunks::<32>().0.iter();
    let recovered_pubkey_bytes = unsafe {
        let digest = it.next().unwrap_unchecked();
        let v = it.next().unwrap_unchecked();
        let r = it.next().unwrap_unchecked();
        let s = it.next().unwrap_unchecked();

        if v[..31].iter().all(|el| *el == 0) == false {
            return Ok(());
        }

        let rec_id = v[31].wrapping_sub(27);
        if (rec_id == 0 || rec_id == 1) == false {
            return Ok(());
        }

        let Ok(pk_bytes) = ecrecover_inner_with_oracle(digest, r, s, rec_id, oracle, logger) else {
            return Ok(());
        };

        pk_bytes
    };
    let bytes_ref = recovered_pubkey_bytes.as_ref();

    use crypto::sha3::Keccak256;
    use crypto::MiniDigest;
    let address_hash = Keccak256::digest(&bytes_ref[1..]);

    dst.extend(core::iter::repeat_n(0, 12).chain(address_hash.into_iter().skip(12)));

    Ok(())
}

#[cfg(target_arch = "riscv32")]
pub fn ecrecover_inner_with_oracle<
    O: zk_ee::system_io_oracle::IOOracle,
    L: zk_ee::system::logger::Logger
>(
    digest: &[u8; 32],
    r: &[u8; 32],
    s: &[u8; 32],
    rec_id: u8,
    oracle: &mut O,
    _logger: &mut L,
) -> Result<crypto::k256::EncodedPoint, ()> {
    use crate::system_functions::{FIELD_OPS_ADVISE_QUERY_ID, FieldHintOp, FieldOpsHint};
    use crypto::k256::{
        ecdsa::{hazmat::bits2field, RecoveryId, Signature},
        elliptic_curve::ops::Reduce,
        Scalar as K256Scalar,
    };
    use crypto::secp256k1::{ecmult, Scalar, FieldElement, Affine, ECRECOVER_CONTEXT};
    use zk_ee::utils::Bytes32;

    let signature = Signature::from_scalars(*r, *s).map_err(|_| ())?;
    let recovery_id = RecoveryId::try_from(rec_id).map_err(|_| ())?;

    let message = <K256Scalar as Reduce<crypto::k256::U256>>::reduce_bytes(
        &bits2field::<crypto::k256::Secp256k1>(digest).map_err(|_| ())?,
    );

    let (mut sigr, mut sigs) = Scalar::from_signature(&signature);
    let message = Scalar::from_k256_scalar(message);

    // We go through bytes because it's mod GROUP_ORDER and later we need mod BASE FIELD
    // NOTE: parsing signature above checks that we are already < GROUP ORDER
    let mut brx = sigr.to_repr();

    if recovery_id.is_x_reduced() {
        use crypto::k256::elliptic_curve::{Curve, FieldBytesEncoding};
        use crypto::k256::elliptic_curve::bigint::CheckedAdd;
        match <crypto::k256::U256 as FieldBytesEncoding<crypto::k256::Secp256k1>>::decode_field_bytes(&brx)
            .checked_add(&crypto::k256::Secp256k1::ORDER)
            .into_option()
        {
            Some(restored) => {
                brx = <crypto::k256::U256 as FieldBytesEncoding<crypto::k256::Secp256k1>>::encode_field_bytes(&restored);
            }
            None => return Err(()),
        }
    }

    let is_y_odd = recovery_id.is_y_odd();

    // Here we can use hint to get square root. As -1 is non-residue, then either value has square root, or it's negation

    let x = {
        let x_bytes: [u8; 32] = brx.into();
        let Some(x) = FieldElement::from_bytes(&x_bytes) else {
            return Err(())
        };

        // actually recover
        let mut rhs = x;
        rhs.square_in_place();
        rhs *= x;
        rhs += 7;

        let mut y = if rhs.normalizes_to_zero() {
            FieldElement::ZERO
        } else {
            // now we can ask for a hint
            let input = Bytes32::from_array(rhs.to_bytes().try_into().unwrap());
            let (square_root, should_negate): (Bytes32, bool) = {
                let hint_request = FieldOpsHint {
                    op: FieldHintOp::Secp256k1BaseFieldSqrt as u32,
                    src_ptr: input.as_u8_array_ref().as_ptr().addr() as u32,
                    src_len_u32_words: 8,
                };
                oracle.query_serializable(FIELD_OPS_ADVISE_QUERY_ID, &((&hint_request as *const FieldOpsHint).addr() as u32)).map_err(|_| ())?
            };
            // answer is must be a field element
            let Some(fe) = FieldElement::from_bytes(square_root.as_u8_array_ref()) else {
                return Err(());
            };

            if should_negate == false {
                let mut squared = fe;
                squared.square_in_place();
                squared.sub_in_place(&rhs);
                assert!(squared.normalizes_to_zero());
            } else {
                // we must check that hint was correct
                let mut squared = fe;
                squared.square_in_place();
                squared.add_in_place(&rhs);
                assert!(squared.normalizes_to_zero());

                return Err(());
            }

            fe
        };

        if y.is_odd() != is_y_odd {
            y.negate_in_place(1);
        }

        // SAFETY: we recovered coordinates in a checked manner
        let x = unsafe {
            Affine::from_xy_unchecked(x, y)
        };

        x
    };

    let xj = x.to_jacobian();

    // now we also need to invert r mod scalar field,
    // and we already checked that sigr is not zero,
    // so inverse must always exist

    // This case is unreachable if we were able to decompress the point,
    // but easier to check again
    if sigr.is_zero() {
        return Err(());
    }

    let mut sigr_inv: Scalar = {
        // now we can ask for a hint
        let input = Bytes32::from_array(sigr.to_repr().try_into().unwrap());
        let inverse: Bytes32 = {
            let hint_request = FieldOpsHint {
                op: FieldHintOp::Secp256k1ScalarFieldInverse as u32,
                src_ptr: input.as_u8_array_ref().as_ptr().addr() as u32,
                src_len_u32_words: 8,
            };
            oracle.query_serializable(FIELD_OPS_ADVISE_QUERY_ID, &((&hint_request as *const FieldOpsHint).addr() as u32)).map_err(|_| ())?
        };
        // answer is must be a field element
        use crypto::rust_k256::elliptic_curve::scalar::FromUintUnchecked;
        use crypto::rust_k256::elliptic_curve::Curve;
        use crypto::rust_k256::U256;

        let inverse = U256::from_be_slice(inverse.as_u8_array_ref());
        assert!(inverse < crypto::rust_k256::Secp256k1::ORDER);
        let inverse: Scalar = Scalar::from_k256_scalar(crypto::rust_k256::Scalar::from_uint_unchecked(inverse));
        let mut t = sigr;
        t *= &inverse;
        t = t - Scalar::ONE;
        assert!(t.is_zero());

        inverse
    };

    sigs *= sigr_inv;

    sigr_inv *= message;
    sigr_inv.negate_in_place();

    let pk_jacobian = ecmult(&xj, &sigs, &sigr_inv, &ECRECOVER_CONTEXT);
    if pk_jacobian.z().normalizes_to_zero() {
        // point of infinity, so we can bail
        return Err(());
    }

    // now we can use a hint to get inverse again

    let pk: Affine = {
        let z_inv = {
            // now we can ask for a hint
            let input = Bytes32::from_array(pk_jacobian.z().to_bytes().try_into().unwrap());
            let z_inv: Bytes32 = {
                let hint_request = FieldOpsHint {
                    op: FieldHintOp::Secp256k1BaseFieldInverse as u32,
                    src_ptr: input.as_u8_array_ref().as_ptr().addr() as u32,
                    src_len_u32_words: 8,
                };
                oracle.query_serializable(FIELD_OPS_ADVISE_QUERY_ID, &((&hint_request as *const FieldOpsHint).addr() as u32)).map_err(|_| ())?
            };

            // answer is must be a field element
            let Some(z_inv) = FieldElement::from_bytes(z_inv.as_u8_array_ref()) else {
                return Err(());
            };

            z_inv
        };

        let mut x = z_inv;
        let mut y = z_inv;

        x.square_in_place();
        y *= x;

        x *= pk_jacobian.x();
        y *= pk_jacobian.y();

        let pk = unsafe {
            Affine::from_xy_unchecked(x, y)
        };

        pk
    };

    // represent as bytes, and we do not need compression
    let encoded = pk.to_encoded_point(false);

    Ok(encoded)
}

#[cfg(test)]
mod test {
    use super::*;
    use hex;
    use zk_ee::reference_implementations::BaseResources;
    use zk_ee::reference_implementations::DecreasingNative;
    use zk_ee::system::Resource;

    #[test]
    fn test_geth_ecrecover() {
        let input: [u8; 128] =
            hex::decode("38d18acb67d25c8bb9942764b62f18e17054f66a817bd4295423adf9ed98873e000000000000000000000000000000000000000000000000000000000000001b38d18acb67d25c8bb9942764b62f18e17054f66a817bd4295423adf9ed98873e789d1dd423d25f0772d2748d60f7e4b81bb14d086eba8e8e8efb6dcff8a4ae02")
                .expect("should decode hex")
                .try_into()
                .unwrap();

        let expected_pubkey: [u8; 32] =
            hex::decode("000000000000000000000000ceaccac640adf55b2028469bd36ba501f28b699d")
                .expect("should decode pubkey")
                .try_into()
                .unwrap();

        let mut pubkey = vec![];

        let mut resources = <BaseResources<DecreasingNative> as Resource>::FORMAL_INFINITE;

        ecrecover_as_system_function_inner(input.as_slice(), &mut pubkey, &mut resources)
            .expect("ecrecover");
        assert_eq!(pubkey.len(), 32, "Size should be 32");
        assert_eq!(
            pubkey, expected_pubkey,
            "pubkey should be equal to reference"
        )
    }

    #[test]
    fn test_empty_input() {
        let input = [0u8; 128];
        let mut pubkey = vec![];

        let mut resources = <BaseResources<DecreasingNative> as Resource>::FORMAL_INFINITE;

        ecrecover_as_system_function_inner(input.as_slice(), &mut pubkey, &mut resources)
            .expect("ecrecover");
        assert_eq!(pubkey.len(), 0, "Size should be 0");
    }

    #[test]
    fn test_point_of_infinity_in_result() {
        let input: [u8; 128] =
            hex::decode("6b8d2c81b11b2d699528dde488dbdf2f94293d0d33c32e347f255fa4a6c1f0a9000000000000000000000000000000000000000000000000000000000000001b79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f817986b8d2c81b11b2d699528dde488dbdf2f94293d0d33c32e347f255fa4a6c1f0a9")
                .expect("should decode hex")
                .try_into()
                .unwrap();

        let mut pubkey = vec![];

        let mut resources = <BaseResources<DecreasingNative> as Resource>::FORMAL_INFINITE;

        ecrecover_as_system_function_inner(input.as_slice(), &mut pubkey, &mut resources)
            .expect("ecrecover");
        assert_eq!(pubkey.len(), 0, "Size should be 0 in case of error");
    }

    #[test]
    fn test_affine_point_decompression_regression() {
        let input: [u8; 128] =
            hex::decode("00c547e4f7b0f325ad1e56f57e26c745b09a3e503d86e00e5255ff7f715d3d1c000000000000000000000000000000000000000000000000000000000000001c00b1693892219d736caba55bdb67216e485557ea6b6af75f37096c9aa6a5a75f00b940b1d03b21e36b0e47e79769f095fe2ab855bd91e3a38756b7d75a9c4549")
                .expect("should decode hex")
                .try_into()
                .unwrap();

        let mut pubkey = vec![];

        let mut resources = <BaseResources<DecreasingNative> as Resource>::FORMAL_INFINITE;

        ecrecover_as_system_function_inner(input.as_slice(), &mut pubkey, &mut resources)
            .expect("ecrecover");
        assert_eq!(pubkey.len(), 0, "Size should be 0 in case of error");
    }

    #[test]
    fn test_regressions() {
        let input: [u8; 128] = [
            34, 189, 7, 49, 212, 191, 250, 136, 64, 38, 37, 181, 186, 57, 224, 78, 233, 173, 214,
            83, 76, 49, 218, 108, 17, 157, 130, 90, 57, 130, 43, 41, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 28, 102, 102, 116, 99,
            212, 10, 196, 65, 102, 33, 136, 237, 62, 102, 50, 156, 33, 172, 161, 101, 19, 51, 146,
            204, 26, 20, 184, 68, 133, 96, 10, 135, 80, 135, 255, 193, 105, 5, 204, 108, 234, 239,
            23, 70, 48, 206, 157, 208, 196, 11, 63, 78, 148, 255, 0, 238, 54, 88, 166, 166, 127,
            236, 38, 19,
        ];
        let mut pubkey = vec![];
        let mut resources = <BaseResources<DecreasingNative> as Resource>::FORMAL_INFINITE;

        let expected_pubkey: [u8; 32] = [
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 99, 249, 114, 95, 16, 115, 88, 201, 17, 91, 201,
            216, 108, 114, 221, 88, 35, 233, 177, 230,
        ];

        ecrecover_as_system_function_inner(input.as_slice(), &mut pubkey, &mut resources)
            .expect("ecrecover");
        assert_eq!(pubkey.len(), 32, "Size should be 32");
        assert_eq!(
            pubkey, expected_pubkey,
            "pubkey should be equal to reference"
        )
    }
}

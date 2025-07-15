use crate::{define_subsystem, internal_error};

use super::{
    errors::subsystem::{Subsystem, SubsystemError},
    Resources,
};

// Definitions of errors for all system functions
define_subsystem!(Keccak256);
define_subsystem!(Sha256);
define_subsystem!(Secp256k1ECRecover);
define_subsystem!(Secp256k1AddProjective);
define_subsystem!(Secp256k1MulProjective);
define_subsystem!(Secp256r1AddProjective);
define_subsystem!(Secp256r1MulProjective);
define_subsystem!(P256Verify,
                  interface P256VerifyInterfaceError
                  {
                      InvalidInputLength
                  }
);

define_subsystem!(Bn254Add,
                  interface Bn254AddInterfaceError
                  {
                      InvalidPoint
                  }
);

define_subsystem!(Bn254Mul,
                  interface Bn254MulInterfaceError
                  {
                      InvalidPoint
                  }
);
define_subsystem!(Bn254PairingCheck,
                  interface Bn254PairingCheckInterfaceError
                  {
                      InvalidPoint,
                      InvalidPairingSize
                  }
);

define_subsystem!(RipeMd160);

define_subsystem!(ModExp,
                  interface ModExpInterfaceError
                  {
                      InvalidInputLength,
                      InvalidModulus,
                      DivisionByZero
                  }
);

define_subsystem!(MissingSystemFunction);

///
/// System function implementation.
///
pub trait SystemFunction<R: Resources, E: Subsystem> {
    /// Writes result to the `output` and returns actual output slice length that was used.
    /// Should return error on invalid inputs and if resources do not even cover basic parsing cost.
    /// in practice only pairing can have invalid input(size) on charging stage.
    fn execute<D: Extend<u8> + ?Sized, A: core::alloc::Allocator + Clone>(
        input: &[u8],
        output: &mut D,
        resources: &mut R,
        allocator: A,
    ) -> Result<(), SubsystemError<E>>;
}

///
/// System function implementation.
///
pub trait SystemFunctionExt<R: Resources, E: Subsystem> {
    /// Writes result to the `output` and returns actual output slice length that was used.
    /// Should return error on invalid inputs and if resources do not even cover basic parsing cost.
    /// in practice only pairing can have invalid input(size) on charging stage.
    fn execute<O: IOOracle, L: Logger, D: Extend<u8> + ?Sized, A: core::alloc::Allocator + Clone>(
        input: &[u8],
        output: &mut D,
        resources: &mut R,
        oracle: &mut O,
        logger: &mut L,
        allocator: A,
    ) -> Result<(), SubsystemError<E>>;
}

pub struct MissingSystemFunction;

impl<R: Resources> SystemFunction<R, MissingSystemFunctionErrors> for MissingSystemFunction {
    fn execute<D: ?Sized + Extend<u8>, A: core::alloc::Allocator + Clone>(
        _: &[u8],
        _: &mut D,
        _: &mut R,
        _: A,
    ) -> Result<(), SubsystemError<MissingSystemFunctionErrors>> {
        Err(internal_error!("This system function is not defined for this system").into())
    }
}

// Additional implementations for missing projective curve operations
impl<R: Resources> SystemFunction<R, Secp256k1AddProjectiveErrors> for MissingSystemFunction {
    fn execute<D: ?Sized + Extend<u8>, A: core::alloc::Allocator + Clone>(
        _: &[u8],
        _: &mut D,
        _: &mut R,
        _: A,
    ) -> Result<(), SubsystemError<Secp256k1AddProjectiveErrors>> {
        Err(internal_error!("Secp256k1 add projective not implemented").into())
    }
}

impl<R: Resources> SystemFunction<R, Secp256k1MulProjectiveErrors> for MissingSystemFunction {
    fn execute<D: ?Sized + Extend<u8>, A: core::alloc::Allocator + Clone>(
        _: &[u8],
        _: &mut D,
        _: &mut R,
        _: A,
    ) -> Result<(), SubsystemError<Secp256k1MulProjectiveErrors>> {
        Err(internal_error!("Secp256k1 mul projective not implemented").into())
    }
}

impl<R: Resources> SystemFunction<R, Secp256r1AddProjectiveErrors> for MissingSystemFunction {
    fn execute<D: ?Sized + Extend<u8>, A: core::alloc::Allocator + Clone>(
        _: &[u8],
        _: &mut D,
        _: &mut R,
        _: A,
    ) -> Result<(), SubsystemError<Secp256r1AddProjectiveErrors>> {
        Err(internal_error!("Secp256r1 add projective not implemented").into())
    }
}

impl<R: Resources> SystemFunction<R, Secp256r1MulProjectiveErrors> for MissingSystemFunction {
    fn execute<D: ?Sized + Extend<u8>, A: core::alloc::Allocator + Clone>(
        _: &[u8],
        _: &mut D,
        _: &mut R,
        _: A,
    ) -> Result<(), SubsystemError<Secp256r1MulProjectiveErrors>> {
        Err(internal_error!("Secp256r1 mul projective not implemented").into())
    }
}

pub trait SystemFunctions<R: Resources> {
    type Keccak256: SystemFunction<R>;
    type Sha256: SystemFunction<R>;
    type Secp256k1ECRecover: SystemFunction<R>;
    type Secp256k1AddProjective: SystemFunction<R>;
    type Secp256k1MulProjective: SystemFunction<R>;
    type Secp256r1AddProjective: SystemFunction<R>;
    type Secp256r1MulProjective: SystemFunction<R>;
    type P256Verify: SystemFunction<R>;
    type Bn254Add: SystemFunction<R>;
    type Bn254Mul: SystemFunction<R>;
    type Bn254PairingCheck: SystemFunction<R>;
    type RipeMd160: SystemFunction<R>;

    fn keccak256<D: Extend<u8> + ?Sized, A: core::alloc::Allocator + Clone>(
        input: &[u8],
        output: &mut D,
        resources: &mut R,
        allocator: A,
    ) -> Result<(), SubsystemError<Keccak256Errors>> {
        Self::Keccak256::execute(input, output, resources, allocator)
    }

    fn sha256<D: Extend<u8> + ?Sized, A: core::alloc::Allocator + Clone>(
        input: &[u8],
        output: &mut D,
        resources: &mut R,
        allocator: A,
    ) -> Result<(), SubsystemError<Sha256Errors>> {
        Self::Sha256::execute(input, output, resources, allocator)
    }

    fn secp256k1_ec_recover<D: Extend<u8> + ?Sized, A: core::alloc::Allocator + Clone>(
        input: &[u8],
        output: &mut D,
        resources: &mut R,
        allocator: A,
    ) -> Result<(), SubsystemError<Secp256k1ECRecoverErrors>> {
        Self::Secp256k1ECRecover::execute(input, output, resources, allocator)
    }

    fn secp256k1_add_projective<D: Extend<u8> + ?Sized, A: core::alloc::Allocator + Clone>(
        input: &[u8],
        output: &mut D,
        resources: &mut R,
        allocator: A,
    ) -> Result<(), SubsystemError<Secp256k1AddProjectiveErrors>> {
        Self::Secp256k1AddProjective::execute(input, output, resources, allocator)
    }

    fn secp256k1_mul_projective<D: Extend<u8> + ?Sized, A: core::alloc::Allocator + Clone>(
        input: &[u8],
        output: &mut D,
        resources: &mut R,
        allocator: A,
    ) -> Result<(), SubsystemError<Secp256k1MulProjectiveErrors>> {
        Self::Secp256k1MulProjective::execute(input, output, resources, allocator)
    }

    fn secp256r1_add_projective<D: Extend<u8> + ?Sized, A: core::alloc::Allocator + Clone>(
        input: &[u8],
        output: &mut D,
        resources: &mut R,
        allocator: A,
    ) -> Result<(), SubsystemError<Secp256r1AddProjectiveErrors>> {
        Self::Secp256r1AddProjective::execute(input, output, resources, allocator)
    }

    fn secp256r1_mul_projective<D: Extend<u8> + ?Sized, A: core::alloc::Allocator + Clone>(
        input: &[u8],
        output: &mut D,
        resources: &mut R,
        allocator: A,
    ) -> Result<(), SubsystemError<Secp256r1MulProjectiveErrors>> {
        Self::Secp256r1MulProjective::execute(input, output, resources, allocator)
    }

    fn p256_verify<D: Extend<u8> + ?Sized, A: core::alloc::Allocator + Clone>(
        input: &[u8],
        output: &mut D,
        resources: &mut R,
        allocator: A,
    ) -> Result<(), SubsystemError<P256VerifyErrors>> {
        Self::P256Verify::execute(input, output, resources, allocator)
    }

    fn bn254_add<D: Extend<u8> + ?Sized, A: core::alloc::Allocator + Clone>(
        input: &[u8],
        output: &mut D,
        resources: &mut R,
        allocator: A,
    ) -> Result<(), SubsystemError<Bn254AddErrors>> {
        Self::Bn254Add::execute(input, output, resources, allocator)
    }

    fn bn254_mul<D: Extend<u8> + ?Sized, A: core::alloc::Allocator + Clone>(
        input: &[u8],
        output: &mut D,
        resources: &mut R,
        allocator: A,
    ) -> Result<(), SubsystemError<Bn254MulErrors>> {
        Self::Bn254Mul::execute(input, output, resources, allocator)
    }

    fn bn254_pairing_check<D: Extend<u8> + ?Sized, A: core::alloc::Allocator + Clone>(
        input: &[u8],
        output: &mut D,
        resources: &mut R,
        allocator: A,
    ) -> Result<(), SubsystemError<Bn254PairingCheckErrors>> {
        Self::Bn254PairingCheck::execute(input, output, resources, allocator)
    }

    fn ripemd160<D: Extend<u8> + ?Sized, A: core::alloc::Allocator + Clone>(
        input: &[u8],
        output: &mut D,
        resources: &mut R,
        allocator: A,
    ) -> Result<(), SubsystemError<RipeMd160Errors>> {
        Self::RipeMd160::execute(input, output, resources, allocator)
    }
}

pub trait SystemFunctionsExt<R: Resources> {
    type ModExp: SystemFunctionExt<R>;

    fn mod_exp<O: IOOracle, L: Logger, D: Extend<u8> + ?Sized, A: core::alloc::Allocator + Clone>(
        input: &[u8],
        output: &mut D,
        resources: &mut R,
        oracle: &mut O,
        logger: &mut L,
        allocator: A,
    ) -> Result<(), SubsystemError<ModExpErrors>> {
        Self::ModExp::execute(input, output, resources, oracle, logger, allocator)
    }
}

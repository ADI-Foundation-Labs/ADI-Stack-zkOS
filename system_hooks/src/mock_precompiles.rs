//! Mocked precompiles needed to pass some tests in the EVM test suite.
//! Not to be used in production.
#[allow(clippy::module_inception)]
#[cfg(feature = "mock-unsupported-precompiles")]
pub(crate) mod mock_precompiles {
    use zk_ee::{
        internal_error,
        system::{
            Resources, SystemFunction, base_system_functions::MissingSystemFunctionErrors,
            errors::subsystem::SubsystemError,
        },
    };

    pub struct Blake;
    impl<R: Resources> SystemFunction<R, MissingSystemFunctionErrors> for Blake {
        fn execute<D: Extend<u8> + ?Sized, A: core::alloc::Allocator + Clone>(
            input: &[u8],
            _output: &mut D,
            _resources: &mut R,
            _allocator: A,
        ) -> Result<(), SubsystemError<MissingSystemFunctionErrors>> {
            if input.len() != 213 {
                return Err(internal_error!("Invalid Blake input length").into());
            }
            Ok(())
        }
    }

    pub struct PointEval;
    impl<R: Resources> SystemFunction<R, MissingSystemFunctionErrors> for PointEval {
        fn execute<D: Extend<u8> + ?Sized, A: core::alloc::Allocator + Clone>(
            input: &[u8],
            _output: &mut D,
            _resources: &mut R,
            _allocator: A,
        ) -> Result<(), SubsystemError<MissingSystemFunctionErrors>> {
            if input.len() != 193 {
                return Err(internal_error!("Invalid PointEval input length").into());
            }
            Ok(())
        }
    }
}

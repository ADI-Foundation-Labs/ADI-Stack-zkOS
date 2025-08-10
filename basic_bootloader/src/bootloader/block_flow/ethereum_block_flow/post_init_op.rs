use super::*;
use zk_ee::system::errors::internal::InternalError;

impl<S: EthereumLikeTypes> PostSystemInitOp<S> for EthereumPostInitOp
where
    S::IO: IOSubsystemExt,
{
    fn post_init_op<Config: BasicBootloaderExecutionConfig>(
        system: &mut System<S>,
        system_functions: &mut HooksStorage<S, <S as SystemTypes>::Allocator>,
    ) -> Result<(), InternalError> {
        system_functions.add_precompiles();

        use system_hooks::addresses_constants::BLAKE_HOOK_ADDRESS_LOW;

        system_functions.add_precompile::<system_hooks::mock_precompiles::mock_precompiles::Blake, MissingSystemFunctionErrors>(
            BLAKE_HOOK_ADDRESS_LOW,
        );

        super::precompiles::blob_eval_precompile::BlobEvaluationPrecompile::initialize_as_hook(
            system,
            system_functions,
        )?;

        Ok(())
    }
}

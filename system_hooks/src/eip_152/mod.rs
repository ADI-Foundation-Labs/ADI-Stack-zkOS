use super::*;
use zk_ee::interface_error;

define_subsystem!(Blake2FPrecompile,
  interface Blake2FPrecompileInterfaceError
  {
      InvalidInputSize,
      InvalidBooleanFlag,
  }
);

use evm_interpreter::ERGS_PER_GAS;

use zk_ee::define_subsystem;

mod impls;
mod mixing_function;
pub use self::impls::Blake2FPrecompile;

pub fn initialize_eip_152<S: EthereumLikeTypes>(hooks_storage: &mut HooksStorage<S, S::Allocator>)
where
    S::IO: IOSubsystemExt,
{
    hooks_storage.add_precompile_from_pure_invocation::<Blake2FPrecompile>(BLAKE_HOOK_ADDRESS_LOW);
}

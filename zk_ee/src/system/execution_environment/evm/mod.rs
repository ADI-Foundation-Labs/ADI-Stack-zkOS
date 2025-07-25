use ruint::aliases::U256;

use crate::{system::SystemTypes, types_config::SystemIOTypesConfig};

pub trait EvmFrameInterface<S: SystemTypes> {
    /// Instruction pointer
    fn instruction_pointer(&self) -> usize;
    /// Resources left
    fn resources(&self) -> &S::Resources;
    /// EVM stack
    fn stack(&self) -> &impl EvmStackInterface;
    /// Caller address
    fn caller(&self) -> <S::IOTypes as SystemIOTypesConfig>::Address;
    /// Callee address
    fn address(&self) -> <S::IOTypes as SystemIOTypesConfig>::Address;
    /// Calldata
    fn calldata(&self) -> &[u8];
    /// Returndata is available from here if it exists
    fn return_data(&self) -> &[u8];
    /// Heap that belongs to this interpreter frame
    fn heap(&self) -> &[u8];
    /// Bytecode
    fn bytecode(&self) -> &[u8];
    /// Call value
    fn call_value(&self) -> &U256;
    /// Is EVM frame static or not.
    fn is_static(&self) -> bool;
    /// Is interpreter frame executing construction code or not.
    fn is_constructor(&self) -> bool;
}

pub trait EvmStackInterface {
    fn to_slice(&self) -> &[U256];
    fn len(&self) -> usize;
    fn peek_n(&self, index: usize) -> Result<&U256, EvmError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvmError {
    OutOfGas,
    /// TODO currently this error is not used
    CodeStoreOutOfGas,
    /// TODO currently this error is not used
    CallTooDeep,
    /// TODO currently this error is not used
    InsufficientBalance,
    /// TODO currently this error is not used
    CreateCollision,
    CreateContractSizeLimit,
    CreateInitcodeSizeLimit,
    InvalidJump,
    ReturnDataOutOfBounds,
    CreateContractStartingWithEF,
    /// TODO currently this error is not used
    NonceOverflow,

    InvalidOpcode(u8),
    StackUnderflow,
    StackOverflow,
    CallNotAllowedInsideStatic,
    StateChangeDuringStaticCall,
    MemoryLimitOOG,
    InvalidOperandOOG,
}

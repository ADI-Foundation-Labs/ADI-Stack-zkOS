use crate::bootloader::block_flow::{PostTxLoopOp, PreTxLoopOp, TxLoopOp};
use zk_ee::types_config::EthereumIOTypesConfig;

use super::*;

pub trait BasicSTF: Sized + SystemTypes
where
    <Self as SystemTypes>::IO: IOSubsystemExt + IOTeardown<Self::IOTypes>,
{
    type BlockDataKeeper;
    type PreTxLoopOp: PreTxLoopOp<Self, PreTxLoopResult = Self::BlockDataKeeper>;
    type TxLoopOp: TxLoopOp<Self, BlockData = Self::BlockDataKeeper>;
    type PostTxLoopOp: PostTxLoopOp<Self, BlockData = Self::BlockDataKeeper>;
}

pub trait EthereumLikeBasicSTF: BasicSTF
where
    Self: EthereumLikeTypes,
    <Self as SystemTypes>::IO: IOSubsystemExt + IOTeardown<EthereumIOTypesConfig>,
{
}

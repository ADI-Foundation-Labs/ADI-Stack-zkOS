use crate::bootloader::block_flow::BlockDataKeeper;
use crate::bootloader::block_flow::{PostTxLoopOp, PreTxLoopOp, TxLoopOp};
use zk_ee::types_config::EthereumIOTypesConfig;

use super::*;

pub trait BasicSTF: Sized + SystemTypes
where
    <Self as SystemTypes>::IO: IOSubsystemExt + IOTeardown<Self::IOTypes>,
{
    type BlockDataKeeper: BlockDataKeeper;
    type PreTxLoopOp: PreTxLoopOp<Self, PreTxLoopResult = Self::BlockDataKeeper>;
    type TxLoopOp: TxLoopOp<Self, BlockDataKeeper = Self::BlockDataKeeper>;
    type PostTxLoopOp: PostTxLoopOp<Self, BlockDataKeeper = Self::BlockDataKeeper>;
}

pub trait EthereumLikeBasicSTF: BasicSTF
where
    Self: EthereumLikeTypes,
    <Self as SystemTypes>::IO: IOSubsystemExt + IOTeardown<EthereumIOTypesConfig>,
{
}

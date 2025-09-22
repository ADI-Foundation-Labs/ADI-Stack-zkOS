use crate::bootloader::block_flow::{
    MetadataInitOp, PostSystemInitOp, PostTxLoopOp, PreTxLoopOp, TxLoopOp,
};
use zk_ee::types_config::EthereumIOTypesConfig;

use super::*;

pub trait BasicSTF: Sized + SystemTypes
where
    <Self as SystemTypes>::IO: IOSubsystemExt + IOTeardown<Self::IOTypes>,
{
    /// State to collect data during tx execution to be used during postop
    type BlockDataKeeper;
    /// State to collect data during tx/block execution to be used later
    type BatchDataKeeper;
    type BlockHeader: 'static + Sized;
    type MetadataOp: MetadataInitOp<Self>;
    type PostSystemInitOp: PostSystemInitOp<Self>;
    type PreTxLoopOp: PreTxLoopOp<Self, PreTxLoopResult = Self::BlockDataKeeper>;
    type TxLoopOp: TxLoopOp<
        Self,
        BlockData = Self::BlockDataKeeper,
        BatchData = Self::BatchDataKeeper,
    >;
    type PostTxLoopOp: PostTxLoopOp<
        Self,
        BlockData = Self::BlockDataKeeper,
        BatchData = Self::BatchDataKeeper,
        BlockHeader = Self::BlockHeader,
    >;
}

pub trait EthereumLikeBasicSTF: BasicSTF
where
    Self: EthereumLikeTypes,
    <Self as SystemTypes>::IO: IOSubsystemExt + IOTeardown<EthereumIOTypesConfig>,
{
}

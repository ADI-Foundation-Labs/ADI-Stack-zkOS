use crate::run::{NextTxResponse, PreimageSource, ReadStorageTree, TxSource};
use basic_system::system_implementation::flat_storage_model::*;
use serde::{Deserialize, Serialize};
use zk_ee::common_structs::derive_flat_storage_key;
use zk_ee::common_structs::ProofData;
use zk_ee::internal_error;
use zk_ee::kv_markers::{StorageAddress, UsizeSerializable};
use zk_ee::oracle::*;
use zk_ee::system::errors::internal::InternalError;
use zk_ee::system::metadata::BlockMetadataFromOracle;
use zk_ee::system_io_oracle::dyn_usize_iterator::DynUsizeIterator;
use zk_ee::system_io_oracle::*;
use zk_ee::types_config::EthereumIOTypesConfig;
use zk_ee::utils::*;

use super::ReadStorage;

#[derive(Debug, Serialize, Deserialize)]
pub struct DebugOracle {
    pub data: Vec<u32>,
    pub curr: usize,
}

impl DebugOracle {
    pub fn make_iter_dyn<'a, M: OracleIteratorTypeMarker>(
        &'a mut self,
        _init_value: M::Params,
    ) -> Result<Box<dyn ExactSizeIterator<Item = usize> + 'static>, InternalError> {
        struct U32Vec {
            pub inner: Vec<u32>,
        };
        impl UsizeSerializable for U32Vec {
            const USIZE_LEN: usize = 0;

            fn iter(&self) -> impl ExactSizeIterator<Item=usize> {
                let len = self.inner.len() / (usize::BITS / u32::BITS) as usize;
                let capacity = self.inner.capacity() / (usize::BITS / u32::BITS) as usize;

                let copy = self.inner.clone();
                let ptr = copy.as_ptr() as *mut usize;
                core::mem::forget(copy);
                unsafe {
                    Vec::<usize>::from_raw_parts(ptr, len, capacity)
                }.into_iter()
            }
        }
        let curr = self.curr;
        let length = self.data[curr] as usize;
        self.curr += length + 1;
        let u32_vec = U32Vec{
            inner: self.data[curr+1..curr+length+1].to_vec(),
        };
        let iterator = DynUsizeIterator::from_owned(u32_vec);

        Ok(Box::new(iterator))
    }
}

impl IOOracle for DebugOracle
{
    type MarkerTiedIterator<'a> = Box<dyn ExactSizeIterator<Item = usize> + 'static>;

    fn create_oracle_access_iterator<M: OracleIteratorTypeMarker>(
        &mut self,
        init_value: M::Params,
    ) -> Result<Self::MarkerTiedIterator<'_>, InternalError> {
        self.make_iter_dyn::<M>(init_value)
    }
}

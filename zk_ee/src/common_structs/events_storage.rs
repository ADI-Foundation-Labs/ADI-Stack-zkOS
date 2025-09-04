use crate::{
    memory::stack_trait::{StackCtor, StackCtorConst},
    system::errors::system::SystemError,
    types_config::{EthereumIOTypesConfig, SystemIOTypesConfig},
    utils::{Bytes32, UsizeAlignedByteBox},
};
use alloc::alloc::Global;
use arrayvec::ArrayVec;
use core::alloc::Allocator;
use ruint::aliases::*;

use super::history_list::HistoryList;

///
/// Generic event content to be saved in the storage
///
#[derive(Clone, Debug)]
pub struct GenericEventContent<const N: usize, IOTypes: SystemIOTypesConfig, A: Allocator = Global>
{
    pub tx_number: u32,
    pub address: IOTypes::Address,
    pub topics: ArrayVec<IOTypes::EventKey, N>,
    pub data: UsizeAlignedByteBox<A>,
}

///
/// Generic event content reference to be passed into the system during emit
///
#[derive(Clone, Debug)]
pub struct GenericEventContentRef<'a, const N: usize, IOTypes: SystemIOTypesConfig> {
    // NOTE: sender doesn't know TX number
    pub address: &'a IOTypes::Address,
    pub topics: &'a ArrayVec<IOTypes::EventKey, N>,
    pub data: &'a [u8],
}

///
/// Generic event content reference to be returned from the storage
///
#[derive(Clone, Debug)]
pub struct GenericEventContentWithTxRef<'a, const N: usize, IOTypes: SystemIOTypesConfig> {
    pub tx_number: u32,
    pub address: &'a IOTypes::Address,
    pub topics: &'a ArrayVec<IOTypes::EventKey, N>,
    pub data: &'a [u8],
}

#[allow(type_alias_bounds)]
pub type EventContent<const N: usize, A: Allocator = Global> =
    GenericEventContent<N, EthereumIOTypesConfig, A>;

pub type EventStorageStackCheck<SCC: const StackCtorConst, A: Allocator, const N: usize> = [[();
    SCC::extra_const_param::<(EventContent<N, A>, ()), A>()];
    SCC::extra_const_param::<usize, A>()];

pub struct EventsStorage<
    const N: usize,
    SC: StackCtor<SCC>,
    SCC: const StackCtorConst,
    A: Allocator + Clone = Global,
> where
    EventStorageStackCheck<SCC, A, N>:,
{
    list: HistoryList<EventContent<N, A>, (), SC, SCC, A>,
    _marker: core::marker::PhantomData<A>,
}

impl<const N: usize, SC: StackCtor<SCC>, SCC: const StackCtorConst, A: Allocator + Clone>
    EventsStorage<N, SC, SCC, A>
where
    EventStorageStackCheck<SCC, A, N>:,
{
    pub fn new_from_parts(allocator: A) -> Self {
        Self {
            list: HistoryList::new(allocator),
            _marker: core::marker::PhantomData,
        }
    }

    pub fn begin_new_tx(&mut self) {}

    #[track_caller]
    pub fn start_frame(&mut self) -> usize {
        self.list.snapshot()
    }

    pub fn push_event(
        &mut self,
        tx_number: u32,
        address: &B160,
        topics: &ArrayVec<Bytes32, N>,
        data: UsizeAlignedByteBox<A>,
    ) -> Result<(), SystemError> {
        self.list.push(
            EventContent {
                tx_number,
                address: *address,
                topics: topics.clone(),
                data,
            },
            (),
        );

        Ok(())
    }

    #[track_caller]
    pub fn finish_frame(&mut self, rollback_handle: Option<usize>) {
        if let Some(x) = rollback_handle {
            self.list.rollback(x);
        }
    }

    pub fn iter_net_diff(
        &self,
    ) -> impl Iterator<Item = &GenericEventContent<N, EthereumIOTypesConfig, A>> {
        self.list.iter()
    }

    pub fn events_ref_iter(
        &self,
    ) -> impl Iterator<Item = GenericEventContentWithTxRef<{ N }, EthereumIOTypesConfig>> {
        self.list.iter().map(|event| GenericEventContentWithTxRef {
            tx_number: event.tx_number,
            address: &event.address,
            topics: &event.topics,
            data: event.data.as_slice(),
        })
    }
}

use zksync_os_interface::constants::MAX_EVENT_TOPICS;
impl From<&GenericEventContent<MAX_EVENT_TOPICS, EthereumIOTypesConfig>> for zksync_os_interface::common_types::Log {
    fn from(value: &GenericEventContent<MAX_EVENT_TOPICS, EthereumIOTypesConfig>) -> Self {
        Self {
            address: value.address,
            topics: value.topics.iter().map(|x| zksync_os_interface::bytes32::Bytes32::from_array(x.as_u8_array())).collect(),
            data: value.data.as_slice().to_vec(),
        }
    }
}
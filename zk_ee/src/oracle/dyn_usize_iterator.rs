use alloc::boxed::Box;

// We want to return kind-of owning iterator for UsizeSerializable,
// and we imply that it'll be used only as Box<dyn _> when returned

pub struct DynUsizeIterator<I: 'static, IT: ExactSizeIterator<Item = usize> + 'static> {
    item: I,
    iterator: Option<IT>,
}

impl<I: 'static, IT: ExactSizeIterator<Item = usize> + 'static> DynUsizeIterator<I, IT> {
    pub fn from_constructor<FN: FnOnce(&'static I) -> IT>(
        item: I,
        closure: FN,
    ) -> Box<dyn ExactSizeIterator<Item = usize> + 'static> {
        // TODO: eventually we will get in-place constructors
        unsafe {
            let mut item = Box::new(Self {
                item,
                iterator: None,
            });
            // now with location being stable, we can life-extend it and take reference
            let static_ref: &'static I = core::mem::transmute(&item.as_ref().item);
            let iterator = (closure)(static_ref);
            item.as_mut().iterator = Some(iterator);

            item as Box<dyn ExactSizeIterator<Item = usize> + 'static>
        }
    }
}

impl<I: 'static, IT: ExactSizeIterator<Item = usize> + 'static> Iterator
    for DynUsizeIterator<I, IT>
{
    type Item = usize;

    fn next(&mut self) -> Option<Self::Item> {
        // Safety: we do not move out of item itself, but we modify iterator (also do not move unless drop)

        let mut should_drop = false;
        let Some(it) = self.iterator.as_mut() else {
            // related access
            return None;
        };
        let result = it.next();
        if ExactSizeIterator::len(it) == 0 {
            should_drop = true;
        }
        if should_drop {
            // cleanup
            drop(self.iterator.take().unwrap());
        }

        result
    }
}

impl<I: 'static, IT: ExactSizeIterator<Item = usize> + 'static> ExactSizeIterator
    for DynUsizeIterator<I, IT>
{
    fn len(&self) -> usize {
        self.iterator.as_ref().map(|it| it.len()).unwrap_or(0)
    }
}

impl<I: 'static, IT: ExactSizeIterator<Item = usize> + 'static> Drop for DynUsizeIterator<I, IT> {
    fn drop(&mut self) {
        // we do not move, so iterating is ok
        drop(self.iterator.take());
    }
}

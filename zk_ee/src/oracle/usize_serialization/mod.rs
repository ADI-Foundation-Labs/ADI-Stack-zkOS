use core::mem::MaybeUninit;

use crate::system::errors::internal::InternalError;

pub trait UsizeSerializable {
    const USIZE_LEN: usize;

    fn iter(&self) -> impl ExactSizeIterator<Item = usize>;
}

// Only serializable will have default impl
impl<T: UsizeSerializable, const N: usize> UsizeSerializable for [T; N] {
    const USIZE_LEN: usize = <T as UsizeSerializable>::USIZE_LEN * N;
    fn iter(&self) -> impl ExactSizeIterator<Item = usize> {
        ExactSizeChainN::<_, _, N>::new(
            core::iter::empty::<usize>(),
            core::array::from_fn(|i| Some(UsizeSerializable::iter(&self[i]))),
        )
    }
}

pub trait UsizeDeserializable: Sized {
    const USIZE_LEN: usize;

    fn from_iter(src: &mut impl ExactSizeIterator<Item = usize>) -> Result<Self, InternalError>;

    ///
    /// # Safety
    ///
    /// The correct layout of the serialization is enforced by the `from_iter`
    /// implementation, as long as the data in the external storage is correctly populated. It is a
    /// UB to read from any location that wasn't populated by this type before.
    ///
    unsafe fn init_from_iter(
        this: &mut MaybeUninit<Self>,
        src: &mut impl ExactSizeIterator<Item = usize>,
    ) -> Result<(), InternalError> {
        let new = UsizeDeserializable::from_iter(src)?;
        this.write(new);

        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct ExactSizeChain<A, B> {
    // These are "fused" with `Option` so we don't need separate state to track which part is
    // already exhausted, and we may also get niche layout for `None`. We don't use the real `Fuse`
    // adapter because its specialization for `FusedIterator` unconditionally descends into the
    // iterator, and that could be expensive to keep revisiting stuff like nested chains. It also
    // hurts compiler performance to add more iterator layers to `Chain`.
    //
    // Only the "first" iterator is actually set `None` when exhausted, depending on whether you
    // iterate forward or backward. If you mix directions, then both sides may be `None`.
    a: Option<A>,
    b: Option<B>,
}
impl<A, B> ExactSizeChain<A, B> {
    pub fn new(a: A, b: B) -> ExactSizeChain<A, B> {
        ExactSizeChain {
            a: Some(a),
            b: Some(b),
        }
    }
}

impl<A, B> Iterator for ExactSizeChain<A, B>
where
    A: ExactSizeIterator,
    B: ExactSizeIterator<Item = A::Item>,
{
    type Item = A::Item;

    #[inline]
    fn next(&mut self) -> Option<A::Item> {
        and_then_or_clear(&mut self.a, Iterator::next).or_else(|| self.b.as_mut()?.next())
    }
}

impl<A, B> ExactSizeIterator for ExactSizeChain<A, B>
where
    A: ExactSizeIterator,
    B: ExactSizeIterator<Item = A::Item>,
{
    fn len(&self) -> usize {
        self.a.as_ref().map(|el| el.len()).unwrap_or(0)
            + self.b.as_ref().map(|el| el.len()).unwrap_or(0)
    }
}

#[inline]
fn and_then_or_clear<T, U>(opt: &mut Option<T>, f: impl FnOnce(&mut T) -> Option<U>) -> Option<U> {
    let x = f(opt.as_mut()?);
    if x.is_none() {
        *opt = None;
    }
    x
}

#[derive(Clone, Debug)]
pub struct ExactSizeChainN<A, B, const N: usize> {
    // These are "fused" with `Option` so we don't need separate state to track which part is
    // already exhausted, and we may also get niche layout for `None`. We don't use the real `Fuse`
    // adapter because its specialization for `FusedIterator` unconditionally descends into the
    // iterator, and that could be expensive to keep revisiting stuff like nested chains. It also
    // hurts compiler performance to add more iterator layers to `Chain`.
    //
    // Only the "first" iterator is actually set `None` when exhausted, depending on whether you
    // iterate forward or backward. If you mix directions, then both sides may be `None`.
    a: Option<A>,
    b: [Option<B>; N],
    b_idx: usize,
}

impl<A, B, const N: usize> ExactSizeChainN<A, B, N> {
    pub fn new(a: A, b: [Option<B>; N]) -> Self {
        assert!(N > 0);
        Self {
            a: Some(a),
            b,
            b_idx: 0,
        }
    }
}

impl<A, B, const N: usize> Iterator for ExactSizeChainN<A, B, N>
where
    A: ExactSizeIterator,
    B: ExactSizeIterator<Item = A::Item>,
{
    type Item = A::Item;

    #[inline]
    fn next(&mut self) -> Option<A::Item> {
        if N == 0 {
            and_then_or_clear(&mut self.a, Iterator::next)
        } else {
            and_then_or_clear(&mut self.a, Iterator::next).or_else(|| {
                while self.b_idx < N {
                    if let Some(next) = self.b[self.b_idx].as_mut().unwrap().next() {
                        return Some(next);
                    } else {
                        self.b[self.b_idx] = None;
                        self.b_idx += 1
                    }
                }

                None
            })
        }
    }
}

impl<A, B, const N: usize> ExactSizeIterator for ExactSizeChainN<A, B, N>
where
    A: ExactSizeIterator,
    B: ExactSizeIterator<Item = A::Item>,
{
    fn len(&self) -> usize {
        let mut result = self.a.as_ref().map(|el| el.len()).unwrap_or(0);
        for el in self.b.iter().skip(self.b_idx) {
            result += el.as_ref().map(|el| el.len()).unwrap_or(0)
        }

        result
    }
}

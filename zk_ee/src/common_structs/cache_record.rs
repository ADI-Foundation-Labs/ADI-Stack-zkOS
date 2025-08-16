//! Wraps values with additional metadata used by IO caches

use crate::system::errors::{internal::InternalError, system::SystemError};
use core::fmt::Debug;

#[derive(Default, Copy, Clone, Eq, PartialEq, Debug)]
/// Encodes state of cache element
pub enum Appearance {
    #[default]
    /// Represent uninitialized element - it doesn't exist in persistent form, so it it would be modified
    /// into non-trivial state, then it would need to be persisted as "insert"
    Unset,
    /// Populated with some preexisted value
    Retrieved,
    /// Represent kind-of uninitialized element - it may or may not exist in persistent form, but it was "declared"
    /// to be in cache for some reason, but was not yet read (observed)
    Touched,
    /// Represent the value that was "observed", but maybe was not modified
    Observed,
    /// Cache value was potentially changed compared to initial value
    Updated,
    /// Used for destructed accounts
    Deconstructed,
}

#[derive(Clone, Default)]
/// A cache entry. Wraps actual value with some metadata used by caches.
pub struct CacheRecord<V, M> {
    appearance: Appearance,
    value: V,
    metadata: M,
}

impl<V, M: Default> CacheRecord<V, M> {
    pub fn new(value: V, appearance: Appearance) -> Self {
        Self {
            appearance,
            value,
            metadata: Default::default(),
        }
    }
}

impl<V, M> CacheRecord<V, M> {
    pub fn appearance(&self) -> Appearance {
        self.appearance
    }

    pub fn value(&self) -> &V {
        &self.value
    }

    pub fn metadata(&self) -> &M {
        &self.metadata
    }

    #[must_use]
    /// Updates value and metadata using callback. Changes appearance to Updated.
    pub fn update<F>(&mut self, f: F) -> Result<(), InternalError>
    where
        F: FnOnce(&mut V, &mut M) -> Result<(), InternalError>,
    {
        if self.appearance != Appearance::Deconstructed {
            self.appearance = Appearance::Updated
        };

        f(&mut self.value, &mut self.metadata)
    }

    #[must_use]
    /// Updates the metadata and retains the appearance.
    pub fn update_metadata<F>(&mut self, f: F) -> Result<(), SystemError>
    where
        F: FnOnce(&mut M) -> Result<(), SystemError>,
    {
        f(&mut self.metadata)
    }

    /// Sets appearance to deconstructed. The value itself remains untouched.
    pub fn deconstruct(&mut self) {
        self.appearance = Appearance::Deconstructed;
    }

    /// Sets appearance to unset. The value itself remains untouched.
    pub fn unset(&mut self) {
        self.appearance = Appearance::Unset;
    }

    /// Sets appearance to "touched" to distinguish from elements that were "observed" via explicit read
    /// or update. If it was observed before - does nothing
    pub fn touch(&mut self) {
        if self.appearance == Appearance::Unset || self.appearance == Appearance::Retrieved {
            self.appearance = Appearance::Touched;
        };
    }

    /// Sets appearance to "observed" to distinguish from elements that were "observed" via explicit read
    /// or update. If it was observed before - does nothing
    pub fn observe(&mut self) {
        if self.appearance == Appearance::Unset || self.appearance == Appearance::Retrieved {
            self.appearance = Appearance::Observed;
        };
    }
}

#[cfg(test)]
mod tests {
    use super::{Appearance, CacheRecord};

    #[test]
    fn update_works_and_changes_appearance() {
        let mut cache_record: CacheRecord<i32, u32> = CacheRecord::new(5, Appearance::Retrieved);
        cache_record
            .update(|v, _| {
                *v = 4;
                Ok(())
            })
            .expect("Correct update");

        assert_eq!(cache_record.value, 4);
        assert_eq!(cache_record.appearance, Appearance::Updated);
    }

    #[test]
    fn metadata_update_keeps_appearance() {
        let mut cache_record: CacheRecord<i32, u32> = CacheRecord::new(5, Appearance::Retrieved);
        cache_record
            .update_metadata(|m| {
                *m = 4;
                Ok(())
            })
            .expect("Correct update");

        assert_eq!(cache_record.appearance, Appearance::Retrieved);
    }

    #[test]
    fn deconstruct_works() {
        let mut cache_record: CacheRecord<i32, u32> = CacheRecord::new(5, Appearance::Retrieved);
        cache_record.deconstruct();

        assert_eq!(cache_record.appearance, Appearance::Deconstructed);
    }

    #[test]
    fn unset_works() {
        let mut cache_record: CacheRecord<i32, u32> = CacheRecord::new(5, Appearance::Retrieved);
        cache_record.unset();

        assert_eq!(cache_record.appearance, Appearance::Unset);
    }
}

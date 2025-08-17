//! Wraps values with additional metadata used by IO caches

use crate::system::errors::{internal::InternalError, system::SystemError};
use core::fmt::Debug;

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
/// Encodes state of cache element
pub enum InitialAppearance {
    /// Represent uninitialized element - it doesn't exist in persistent form, so it it would be modified
    /// into non-trivial state, then it would need to be persisted as "insert"
    Unset,
    /// Populated with some preexisted value
    Retrieved,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
/// Encodes state of cache element
pub enum CurrentAppearance {
    /// Represent kind-of uninitialized element - it may or may not exist in persistent form, but it was "declared"
    /// to be in cache for some reason, but was not yet read (observed)
    Touched,
    /// Represent the value that was "observed", but maybe was not modified
    Observed,
    /// Cache value was potentially changed compared to initial value
    Updated,
    /// Marks an account that is deconstructed according to EIP-6780 "in same transaction" rules
    MarkedForDeconstruction,
    /// Account deconstruction was completed at the end of transaction
    Deconstructed,
}

#[derive(Clone)]
/// A cache entry. Wraps actual value with some metadata used by caches.
pub struct StructuredCacheRecord<V, M> {
    initial_appearance: InitialAppearance,
    current_appearance: CurrentAppearance,
    value: V,
    metadata: M,
}

impl<V, M: Default> StructuredCacheRecord<V, M> {
    pub fn new(
        value: V,
        initial_appearance: InitialAppearance,
        current_appearance: CurrentAppearance,
    ) -> Self {
        Self {
            initial_appearance,
            current_appearance,
            value,
            metadata: Default::default(),
        }
    }
}

impl<V, M> StructuredCacheRecord<V, M> {
    pub fn initial_appearance(&self) -> InitialAppearance {
        self.initial_appearance
    }
    pub fn current_appearance(&self) -> CurrentAppearance {
        self.current_appearance
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
        if self.current_appearance != CurrentAppearance::Deconstructed {
            self.current_appearance = CurrentAppearance::Updated;
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
    pub fn mark_for_deconstruction(&mut self) {
        self.current_appearance = CurrentAppearance::MarkedForDeconstruction;
    }

    /// Finishes deconstruction (that is in-same-tx by EIP-6780 rules), and marks an account as just observed
    pub fn finish_deconstruction(&mut self) {
        if self.current_appearance == CurrentAppearance::MarkedForDeconstruction {
            self.current_appearance = CurrentAppearance::Deconstructed;
        }
    }

    /// Sets appearance to "observed" to distinguish from elements that were "observed" via explicit read
    /// or update. If it was observed before - does nothing
    pub fn observe(&mut self) {
        if self.current_appearance == CurrentAppearance::Touched {
            self.current_appearance = CurrentAppearance::Observed;
        };
    }

    /// There can be a case if in two transactions one will do 1) create/self-destruct 2) create to same address again
    /// In this case we should avoid permanently locking an account in the "deconstructed" state
    pub fn mark_as_created(&mut self) {
        if self.current_appearance == CurrentAppearance::Deconstructed {
            self.current_appearance = CurrentAppearance::Updated;
        }
    }
}

//! Wraps values with additional metadata used by IO caches

use crate::common_structs::StructuredCacheAppearance;
use core::fmt::Debug;

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
/// Encodes state of cache element
pub enum AccountInitialAppearance {
    /// Represent uninitialized element - it doesn't exist in persistent form, so it it would be modified
    /// into non-trivial state, then it would need to be persisted as "insert"
    Unset,
    /// Populated with some preexisted value
    Retrieved,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
/// Encodes state of cache element
pub enum AccountCurrentAppearance {
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

impl AccountCurrentAppearance {
    /// Mark as observed if it was only touched before
    pub fn observe(&mut self) {
        if *self == AccountCurrentAppearance::Touched {
            *self = AccountCurrentAppearance::Observed;
        };
    }
}

#[derive(Clone, Copy, Debug)]
pub struct AccountCacheAppearance {
    initial_appearance: AccountInitialAppearance,
    current_appearance: AccountCurrentAppearance,
}

impl AccountCacheAppearance {
    pub fn new(
        initial_appearance: AccountInitialAppearance,
        current_appearance: AccountCurrentAppearance,
    ) -> Self {
        Self {
            initial_appearance,
            current_appearance,
        }
    }

    /// Sets appearance to deconstructed. The value itself remains untouched.
    pub fn mark_for_deconstruction(&mut self) {
        self.current_appearance = AccountCurrentAppearance::MarkedForDeconstruction;
    }

    /// Finishes deconstruction (that is in-same-tx by EIP-6780 rules), and marks an account as just observed
    pub fn finish_deconstruction(&mut self) {
        if self.current_appearance == AccountCurrentAppearance::MarkedForDeconstruction {
            self.current_appearance = AccountCurrentAppearance::Deconstructed;
        }
    }

    /// Sets appearance to "observed" to distinguish from elements that were "observed" via explicit read
    /// or update. If it was observed before - does nothing
    pub fn observe(&mut self) {
        if self.current_appearance == AccountCurrentAppearance::Touched {
            self.current_appearance = AccountCurrentAppearance::Observed;
        };
    }

    /// Mark element as "update", meaning it was written to, but net difference can be trivial anyway
    pub fn update(&mut self) {
        if self.current_appearance != AccountCurrentAppearance::Deconstructed {
            self.current_appearance = AccountCurrentAppearance::Updated;
        }
    }

    /// There can be a case if in two transactions one will do 1) create/self-destruct 2) create to same address again
    /// In this case we should avoid permanently locking an account in the "deconstructed" state
    pub fn mark_as_created(&mut self) {
        if self.current_appearance == AccountCurrentAppearance::Deconstructed {
            self.current_appearance = AccountCurrentAppearance::Updated;
        }
    }
}

impl StructuredCacheAppearance for AccountCacheAppearance {
    type InitialAppearance = AccountInitialAppearance;
    type CurrentAppearance = AccountCurrentAppearance;

    fn initial_appearance(&self) -> Self::InitialAppearance {
        self.initial_appearance
    }

    fn current_appearance(&self) -> Self::CurrentAppearance {
        self.current_appearance
    }

    fn update_current_appearance<FN: FnOnce(&mut Self::CurrentAppearance) -> ()>(
        &mut self,
        update_fn: FN,
    ) {
        update_fn(&mut self.current_appearance);
    }
}

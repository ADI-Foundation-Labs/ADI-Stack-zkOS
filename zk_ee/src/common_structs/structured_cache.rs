pub trait StructuredCacheAppearance {
    type InitialAppearance: 'static + PartialEq + Eq + core::fmt::Debug;
    type CurrentAppearance: 'static + PartialEq + Eq + core::fmt::Debug;

    fn initial_appearance(&self) -> Self::InitialAppearance;
    fn current_appearance(&self) -> Self::CurrentAppearance;

    fn update_current_appearance<FN: FnOnce(&mut Self::CurrentAppearance) -> ()>(
        &mut self,
        update_fn: FN,
    );
}

impl StructuredCacheAppearance for () {
    type InitialAppearance = ();
    type CurrentAppearance = ();

    fn initial_appearance(&self) -> Self::InitialAppearance {
        ()
    }
    fn current_appearance(&self) -> Self::CurrentAppearance {
        ()
    }

    fn update_current_appearance<FN: FnOnce(&mut Self::CurrentAppearance) -> ()>(
        &mut self,
        _update_fn: FN,
    ) {
    }
}

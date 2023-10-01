use std::{rc::Rc, cell::RefCell};

#[derive(Clone)]
pub struct ShieldedStoreImpl {}

impl Default for ShieldedStoreImpl {
    fn default() -> Self {
        Self {}
    }
}

#[derive(Clone, Default)]
pub struct ShieldedStore(pub(crate) Rc<RefCell<ShieldedStoreImpl>>);

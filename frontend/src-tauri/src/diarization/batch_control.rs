//! Offline cancellation is installed only on the owning blocking worker thread.
//! Live inference and unrelated speaker operations never read a global cancel bit.
use std::{cell::Cell, marker::PhantomData, rc::Rc};
thread_local! { static CANCEL: Cell<Option<fn() -> bool>> = const { Cell::new(None) }; }

pub struct Scope {
    previous: Option<fn() -> bool>,
    _thread: PhantomData<Rc<()>>,
}
impl Scope {
    pub fn new(check: Option<fn() -> bool>) -> Self {
        Self {
            previous: CANCEL.with(|slot| slot.replace(check)),
            _thread: PhantomData,
        }
    }
}
impl Drop for Scope {
    fn drop(&mut self) {
        CANCEL.with(|slot| slot.set(self.previous));
    }
}
pub fn check() -> anyhow::Result<()> {
    if CANCEL.with(|slot| slot.get().is_some_and(|check| check())) {
        anyhow::bail!("Retranscription cancelled at an offline inference boundary");
    }
    Ok(())
}

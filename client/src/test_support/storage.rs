//! In-memory credential provider with controllable failures and blocking for storage/session/view tests.
use crate::storage::{Selection, Store, account};
use std::sync::{Arc, Condvar, Mutex};

// Entries, block writes, fail writes, fail deletes, block deletes, deletion started.
pub(crate) type Shared = Arc<(
    Mutex<(Vec<(String, String)>, bool, bool, bool, bool, bool)>,
    Condvar,
)>;
pub(crate) struct Controlled(pub(crate) Shared);
impl Store for Controlled {
    fn get(&mut self, s: &Selection) -> Result<Option<String>, ()> {
        Ok(self
            .0
            .0
            .lock()
            .unwrap()
            .0
            .iter()
            .find(|(k, _)| k == &account(s))
            .map(|(_, v)| v.clone()))
    }
    fn put(&mut self, s: &Selection, t: &str) -> Result<(), ()> {
        let (lock, signal) = &*self.0;
        let mut state = lock.lock().unwrap();
        while state.1 {
            state = signal.wait(state).unwrap();
        }
        if state.2 {
            return Err(());
        }
        state.0.retain(|(k, _)| k != &account(s));
        state.0.push((account(s), t.into()));
        Ok(())
    }
    fn delete(&mut self, s: &Selection) -> Result<(), ()> {
        let (lock, signal) = &*self.0;
        let mut state = lock.lock().unwrap();
        state.5 = true;
        signal.notify_all();
        while state.4 {
            state = signal.wait(state).unwrap();
        }
        if state.3 {
            return Err(());
        }
        state.0.retain(|(k, _)| k != &account(s));
        Ok(())
    }
}

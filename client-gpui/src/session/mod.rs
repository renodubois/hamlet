//! Session ownership home. Coordination remains in the temporary application shell.
//! The state exports preserve existing callers until the session/API ownership tickets.

mod state;

pub(crate) use state::*;

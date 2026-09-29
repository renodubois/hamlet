//! Conversation ownership home. Coordination remains in the temporary application shell.
//! The state exports preserve existing callers until conversation ownership migrates.

pub(crate) mod polling;
mod state;

pub(crate) use state::*;

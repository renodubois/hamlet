//! Owned screen composition, login, sidebar and history; composer extraction remains.

pub(crate) mod app_shell;
mod channel_sidebar;
mod conversation;
pub(crate) mod login;
pub(crate) mod workspace;

#[cfg(test)]
#[path = "tests/login.rs"]
mod login_tests;

#[cfg(test)]
#[path = "tests/workspace.rs"]
mod workspace_tests;

#[cfg(test)]
#[path = "tests/history_lifecycle.rs"]
mod history_lifecycle_tests;

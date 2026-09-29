//! Owned screen composition, login and channel sidebar; history/composer remain combined.

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

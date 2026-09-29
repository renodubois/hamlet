//! Owned views; authenticated controls remain in the temporary shell.

pub(crate) mod app_shell;
pub(crate) mod login;

#[cfg(test)]
#[path = "tests/login.rs"]
mod login_tests;

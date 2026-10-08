pub(crate) mod app_shell;
mod chat;
mod conversation;
pub(crate) mod login;
mod session_footer;
mod sidebar;

#[cfg(test)]
mod tests;

#[cfg(test)]
#[path = "tests/login.rs"]
mod login_tests;

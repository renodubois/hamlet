pub(crate) mod app_shell;
mod conversation;
pub(crate) mod login;
mod session_footer;
mod sidebar;
pub(crate) mod workspace;

#[cfg(test)]
mod tests;

#[cfg(test)]
#[path = "tests/login.rs"]
mod login_tests;

#[cfg(test)]
#[path = "tests/workspace.rs"]
mod workspace_tests;

#[cfg(test)]
#[path = "tests/history_lifecycle.rs"]
mod history_lifecycle_tests;

#[cfg(test)]
#[path = "tests/composer_lifecycle.rs"]
mod composer_lifecycle_tests;

//! Command-line administration helpers for server operators.
//!
//! The CLI intentionally goes directly through the configured SQLite database
//! instead of exposing a remote admin HTTP endpoint. Account creation reuses the
//! normal auth registration path so password hashing and per-user bookkeeping
//! stay centralized.

use thiserror::Error;

use crate::auth;
use crate::database::{DatabaseSetupError, connect_initialized_database_url};
use crate::error::AppError;

pub const USAGE: &str = r#"Usage:
  hamlet-admin create-user --username <username> --password <temporary-password>

Creates a password-backed Hamlet user in the configured server database.
Run with the same DATABASE_URL or HAMLET_DATA_DIR environment that the server uses.

Commands:
  create-user    Create a user account without creating a login session

Options:
  --username <username>                 Username for the new account
  --password <temporary-password>       Temporary password for the new account
  -h, --help                            Show this help
"#;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AdminCommand {
    Help,
    CreateUser { username: String, password: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreatedUser {
    pub id: i64,
    pub username: String,
}

#[derive(Debug, Error)]
pub enum AdminCliError {
    #[error("{0}")]
    Usage(String),
    #[error("username {0:?} already exists")]
    UsernameTaken(String),
    #[error("database setup failed: {0}")]
    Database(#[source] Box<DatabaseSetupError>),
    #[error("account creation failed: {0}")]
    AccountCreation(#[source] Box<AppError>),
}

impl AdminCliError {
    pub fn exit_code(&self) -> i32 {
        match self {
            Self::Usage(_) => 2,
            Self::UsernameTaken(_) | Self::Database(_) | Self::AccountCreation(_) => 1,
        }
    }

    pub fn is_usage(&self) -> bool {
        matches!(self, Self::Usage(_))
    }
}

impl From<DatabaseSetupError> for AdminCliError {
    fn from(error: DatabaseSetupError) -> Self {
        Self::Database(Box::new(error))
    }
}

pub fn parse_args<I, S>(args: I) -> Result<AdminCommand, AdminCliError>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let mut args = args.into_iter().map(Into::into);
    let Some(command) = args.next() else {
        return Err(usage_error("missing command"));
    };

    if is_help_arg(&command) {
        return Ok(AdminCommand::Help);
    }

    if command != "create-user" {
        return Err(usage_error(format!("unknown command {command:?}")));
    }

    let mut username = None;
    let mut password = None;

    while let Some(arg) = args.next() {
        if is_help_arg(&arg) {
            return Ok(AdminCommand::Help);
        }

        if let Some(value) = arg.strip_prefix("--username=") {
            set_once(&mut username, "--username", value.to_owned())?;
            continue;
        }

        if let Some(value) = arg.strip_prefix("--password=") {
            set_once(&mut password, "--password", value.to_owned())?;
            continue;
        }

        match arg.as_str() {
            "--username" => {
                let Some(value) = args.next() else {
                    return Err(usage_error("missing value for --username"));
                };
                set_once(&mut username, "--username", value)?;
            }
            "--password" => {
                let Some(value) = args.next() else {
                    return Err(usage_error("missing value for --password"));
                };
                set_once(&mut password, "--password", value)?;
            }
            _ => return Err(usage_error(format!("unknown argument {arg:?}"))),
        }
    }

    let username = normalize_username(username.ok_or_else(|| usage_error("missing --username"))?)?;
    let password = validate_password(password.ok_or_else(|| usage_error("missing --password"))?)?;

    Ok(AdminCommand::CreateUser { username, password })
}

pub async fn create_user_in_database(
    database_url: &str,
    username: &str,
    password: &str,
) -> Result<CreatedUser, AdminCliError> {
    let username = normalize_username(username.to_owned())?;
    let password = validate_password(password.to_owned())?;
    let db = connect_initialized_database_url(database_url).await?;

    let user = match auth::register_user(&db, &username, &password, None).await {
        Ok(user) => user,
        Err(AppError::UsernameTaken) => return Err(AdminCliError::UsernameTaken(username)),
        Err(error) => return Err(AdminCliError::AccountCreation(Box::new(error))),
    };

    Ok(CreatedUser {
        id: user.id,
        username: user.username,
    })
}

pub async fn run_command(
    command: AdminCommand,
    database_url: &str,
) -> Result<String, AdminCliError> {
    match command {
        AdminCommand::Help => Ok(USAGE.to_owned()),
        AdminCommand::CreateUser { username, password } => {
            let user = create_user_in_database(database_url, &username, &password).await?;
            Ok(success_message(&user))
        }
    }
}

pub fn success_message(user: &CreatedUser) -> String {
    format!("created user {:?} (id: {})", user.username, user.id)
}

fn usage_error(message: impl Into<String>) -> AdminCliError {
    AdminCliError::Usage(message.into())
}

fn is_help_arg(arg: &str) -> bool {
    matches!(arg, "-h" | "--help")
}

fn set_once(slot: &mut Option<String>, flag: &str, value: String) -> Result<(), AdminCliError> {
    if slot.replace(value).is_some() {
        return Err(usage_error(format!("{flag} specified more than once")));
    }
    Ok(())
}

fn normalize_username(username: String) -> Result<String, AdminCliError> {
    let username = username.trim().to_owned();
    if username.is_empty() {
        return Err(usage_error("username must not be blank"));
    }
    Ok(username)
}

fn validate_password(password: String) -> Result<String, AdminCliError> {
    if password.trim().is_empty() {
        return Err(usage_error("password must not be blank"));
    }
    Ok(password)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn parse_create_user_accepts_flags_in_order() {
        assert_eq!(
            parse_args([
                "create-user",
                "--username",
                "alice",
                "--password",
                "hunter2"
            ])
            .unwrap(),
            AdminCommand::CreateUser {
                username: "alice".to_owned(),
                password: "hunter2".to_owned(),
            }
        );
    }

    #[test]
    fn parse_create_user_accepts_equals_flags_and_trims_username() {
        assert_eq!(
            parse_args(["create-user", "--password=hunter2", "--username= alice "]).unwrap(),
            AdminCommand::CreateUser {
                username: "alice".to_owned(),
                password: "hunter2".to_owned(),
            }
        );
    }

    #[test]
    fn parse_help_is_a_command() {
        assert_eq!(parse_args(["--help"]).unwrap(), AdminCommand::Help);
        assert_eq!(
            parse_args(["create-user", "--help"]).unwrap(),
            AdminCommand::Help
        );
    }

    #[test]
    fn parse_rejects_missing_command() {
        assert_usage_error(parse_args(std::iter::empty::<&str>()), "missing command");
    }

    #[test]
    fn parse_rejects_unknown_command() {
        assert_usage_error(parse_args(["delete-user"]), "unknown command");
    }

    #[test]
    fn parse_rejects_unknown_argument() {
        assert_usage_error(
            parse_args(["create-user", "--username", "alice", "--bad", "value"]),
            "unknown argument",
        );
    }

    #[test]
    fn parse_rejects_missing_flag_values() {
        assert_usage_error(
            parse_args(["create-user", "--username"]),
            "missing value for --username",
        );
        assert_usage_error(
            parse_args(["create-user", "--password"]),
            "missing value for --password",
        );
    }

    #[test]
    fn parse_rejects_missing_required_flags() {
        assert_usage_error(
            parse_args(["create-user", "--password", "hunter2"]),
            "missing --username",
        );
        assert_usage_error(
            parse_args(["create-user", "--username", "alice"]),
            "missing --password",
        );
    }

    #[test]
    fn parse_rejects_blank_values() {
        assert_usage_error(
            parse_args(["create-user", "--username", " ", "--password", "hunter2"]),
            "username must not be blank",
        );
        assert_usage_error(
            parse_args(["create-user", "--username", "alice", "--password", " "]),
            "password must not be blank",
        );
    }

    #[test]
    fn parse_rejects_duplicate_flags() {
        assert_usage_error(
            parse_args([
                "create-user",
                "--username",
                "alice",
                "--username",
                "bob",
                "--password",
                "hunter2",
            ]),
            "--username specified more than once",
        );
    }

    #[test]
    fn usage_errors_exit_with_code_two() {
        let err = parse_args(std::iter::empty::<&str>()).unwrap_err();
        assert!(err.is_usage());
        assert_eq!(err.exit_code(), 2);
    }

    fn assert_usage_error(result: Result<AdminCommand, AdminCliError>, needle: &str) {
        let err = result.expect_err("expected usage error");
        match err {
            AdminCliError::Usage(message) => assert!(
                message.contains(needle),
                "expected {message:?} to contain {needle:?}"
            ),
            other => panic!("expected usage error, got {other:?}"),
        }
    }
}

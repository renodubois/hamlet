use std::process::ExitCode;

use hamlet::{Config, admin_cli};

#[tokio::main]
async fn main() -> ExitCode {
    let command = match admin_cli::parse_args(std::env::args().skip(1)) {
        Ok(command) => command,
        Err(error) => return exit_with_error(error),
    };

    if matches!(command, admin_cli::AdminCommand::Help) {
        print!("{}", admin_cli::USAGE);
        return ExitCode::SUCCESS;
    }

    let config = Config::from_env();

    match admin_cli::run_command(command, &config.database_url).await {
        Ok(message) => {
            println!("{message}");
            ExitCode::SUCCESS
        }
        Err(error) => exit_with_error(error),
    }
}

fn exit_with_error(error: admin_cli::AdminCliError) -> ExitCode {
    eprintln!("error: {error}");
    if error.is_usage() {
        eprintln!();
        eprint!("{}", admin_cli::USAGE);
    }
    ExitCode::from(error.exit_code() as u8)
}

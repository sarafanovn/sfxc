#![recursion_limit = "256"]

mod cli;
mod commands;
mod schema;
mod update;

use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use clap::Parser;

fn main() -> ExitCode {
    let cli = cli::Cli::parse();
    let hint = if matches!(cli.command, cli::Command::Update { .. }) { None } else { update::start_hint() };
    let code = match run(cli) {
        Ok(value) => {
            println!("{}", serde_json::to_string_pretty(&value).expect("JSON values always serialize"));
            if value.get("failed").and_then(|f| f.as_u64()).unwrap_or(0) > 0 { ExitCode::FAILURE } else { ExitCode::SUCCESS }
        }
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::FAILURE
        }
    };
    update::finish_hint(hint);
    code
}

fn run(cli: cli::Cli) -> anyhow::Result<serde_json::Value> {
    match cli.command {
        cli::Command::Schema => return Ok(schema::schema()),
        cli::Command::Update { check } => return update::run(check),
        _ => {}
    }
    let store = sfxc_store::Store::open(&sfxc_store::library_path())?;
    let since_epoch = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    let ctx = commands::Ctx { now: since_epoch.as_secs() as i64, seed: since_epoch.as_nanos() as u64, cwd: std::env::current_dir()? };
    commands::run(&store, cli.command, &ctx)
}

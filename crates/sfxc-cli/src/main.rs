#![recursion_limit = "256"]

mod cli;
mod commands;
mod schema;

use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use clap::Parser;

fn main() -> ExitCode {
    match run(cli::Cli::parse()) {
        Ok(value) => {
            println!("{}", serde_json::to_string_pretty(&value).expect("JSON values always serialize"));
            if value.get("failed").and_then(|f| f.as_u64()).unwrap_or(0) > 0 { ExitCode::FAILURE } else { ExitCode::SUCCESS }
        }
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: cli::Cli) -> anyhow::Result<serde_json::Value> {
    if let cli::Command::Schema = cli.command {
        return Ok(schema::schema());
    }
    let store = sfxc_store::Store::open(&sfxc_store::library_path())?;
    let since_epoch = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    let ctx = commands::Ctx { now: since_epoch.as_secs() as i64, seed: since_epoch.as_nanos() as u64, cwd: std::env::current_dir()? };
    commands::run(&store, cli.command, &ctx)
}

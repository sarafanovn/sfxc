use std::io::IsTerminal;
use std::path::PathBuf;
use std::thread::JoinHandle;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Result};
use serde_json::{json, Value};
use sfxc_update::{Part, CURRENT};

/// `sfxc-cli update [--check]`: always asks GitHub (the cache is only for the automatic hint), then installs
/// unless `--check`.
pub fn run(check_only: bool) -> Result<Value> {
    let latest = sfxc_update::fetch_latest(Duration::from_secs(10)).map_err(|e| anyhow!(e))?;
    sfxc_update::write_cache(&sfxc_store::update_cache_path(), now(), latest.as_deref());
    let available = latest.as_deref().is_some_and(|v| sfxc_update::is_newer(v, CURRENT));
    let mut out = json!({ "current": CURRENT, "latest": latest, "update_available": available, "updated": false });
    if !available || check_only {
        return Ok(out);
    }
    sfxc_update::install(Part::Cli).map_err(|e| anyhow!("update failed: {e}"))?;
    let installed = PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".local/bin/sfxc-cli");
    out["updated"] = json!(true);
    out["path"] = json!(installed);
    let running = std::env::current_exe().ok().and_then(|p| p.canonicalize().ok());
    if running.is_some_and(|r| Some(r) != installed.canonicalize().ok()) {
        out["note"] = json!(format!("installed to {}, but this sfxc-cli runs from elsewhere; put ~/.local/bin first on PATH", installed.display()));
    }
    Ok(out)
}

/// Starts the once-a-day check behind the "new version" hint. Only for a person at a terminal: agents read
/// stdout and get no network calls or extra output.
pub fn start_hint() -> Option<JoinHandle<Option<String>>> {
    if sfxc_update::auto_check_disabled() || !std::io::stderr().is_terminal() {
        return None;
    }
    let cache: PathBuf = sfxc_store::update_cache_path();
    std::thread::Builder::new()
        .name("sfxc-update-check".into())
        .spawn(move || sfxc_update::check(&cache, now(), Duration::from_secs(2)))
        .ok()
}

pub fn finish_hint(hint: Option<JoinHandle<Option<String>>>) {
    if let Some(version) = hint.and_then(|h| h.join().ok().flatten()) {
        eprintln!("sfxc-cli {version} is available (you have {CURRENT}). Run `sfxc-cli update`.");
    }
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
}

//! Checks GitHub for a newer sfxc release and installs it with install.sh.
//!
//! Everything goes through the system `curl` and `bash` (both ship with macOS), so no HTTP or TLS stack is linked.
//! `SFXC_NO_UPDATE_CHECK=1` turns off the automatic checks; an explicit `sfxc-cli update` still works.

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

pub const REPO: &str = "sarafanovn/sfxc";
pub const CURRENT: &str = env!("CARGO_PKG_VERSION");

/// Automatic checks ask GitHub at most this often; the answer is cached in between.
pub const CHECK_EVERY: Duration = Duration::from_secs(24 * 60 * 60);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Part {
    App,
    Cli,
}

/// Whether automatic checks are off: by `SFXC_NO_UPDATE_CHECK`, or because this is a debug build.
pub fn auto_check_disabled() -> bool {
    cfg!(debug_assertions) || std::env::var_os("SFXC_NO_UPDATE_CHECK").is_some_and(|v| !v.is_empty() && v != "0")
}

pub fn release_page(version: &str) -> String {
    format!("https://github.com/{REPO}/releases/tag/v{version}")
}

/// The latest published version without the leading `v`; `None` when nothing is released yet.
///
/// `releases/latest` redirects to the newest release's tag page, so one HEAD request is enough and the
/// GitHub API rate limit does not apply.
pub fn fetch_latest(timeout: Duration) -> Result<Option<String>, String> {
    let out = Command::new("curl")
        .args(["-fsSI", "--max-time", &timeout.as_secs().max(1).to_string()])
        .arg(format!("https://github.com/{REPO}/releases/latest"))
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("could not run curl: {e}"))?;
    if !out.status.success() {
        return Err(format!("could not reach GitHub (curl exit {})", out.status.code().unwrap_or(-1)));
    }
    Ok(tag_from_headers(&String::from_utf8_lossy(&out.stdout)))
}

fn tag_from_headers(headers: &str) -> Option<String> {
    let location = headers.lines().find_map(|l| {
        let (name, value) = l.split_once(':')?;
        name.trim().eq_ignore_ascii_case("location").then(|| value.trim())
    })?;
    let tag = location.split_once("/releases/tag/")?.1;
    let tag = tag.strip_prefix('v').unwrap_or(tag);
    (!tag.is_empty()).then(|| tag.to_string())
}

/// Compares dotted versions numerically; a pre-release suffix (`-beta`) is ignored.
pub fn is_newer(latest: &str, current: &str) -> bool {
    fn parts(v: &str) -> Vec<u64> {
        let v = v.split(['-', '+']).next().unwrap_or(v);
        v.split('.').map(|p| p.parse().unwrap_or(0)).collect()
    }
    let (mut a, mut b) = (parts(latest), parts(current));
    let len = a.len().max(b.len());
    a.resize(len, 0);
    b.resize(len, 0);
    a > b
}

/// The cache file holds `<unix seconds of the check> <latest version or empty>`.
fn read_cache(cache: &Path) -> Option<(u64, Option<String>)> {
    let text = std::fs::read_to_string(cache).ok()?;
    let mut it = text.split_whitespace();
    let at = it.next()?.parse().ok()?;
    Some((at, it.next().map(str::to_string)))
}

pub fn write_cache(cache: &Path, now: u64, latest: Option<&str>) {
    if let Some(dir) = cache.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(cache, format!("{now} {}\n", latest.unwrap_or("")));
}

/// Whether the cached answer is recent enough to skip the network.
pub fn cache_is_fresh(cache: &Path, now: u64) -> bool {
    read_cache(cache).is_some_and(|(at, _)| at <= now && now - at < CHECK_EVERY.as_secs())
}

/// A release newer than this build, if there is one. Uses the cache while it is fresh; otherwise asks GitHub
/// and stores the answer. Network errors count as "no update" and are retried on the next call.
pub fn check(cache: &Path, now: u64, timeout: Duration) -> Option<String> {
    let latest = match read_cache(cache) {
        Some((at, latest)) if at <= now && now - at < CHECK_EVERY.as_secs() => latest,
        _ => {
            let latest = fetch_latest(timeout).ok()?;
            write_cache(cache, now, latest.as_deref());
            latest
        }
    };
    latest.filter(|v| is_newer(v, CURRENT))
}

/// Downloads and installs the latest release of `part` with install.sh. On failure the error carries the end of
/// the installer's output.
pub fn install(part: Part) -> Result<(), String> {
    let flag = match part {
        Part::App => "--app",
        Part::Cli => "--cli",
    };
    let script = format!("https://raw.githubusercontent.com/{REPO}/main/install.sh");
    let out = Command::new("bash")
        .args(["-c", r#"set -o pipefail; curl -fsSL "$1" | bash -s -- "$2""#, "sfxc-update", &script, flag])
        .env("SFXC_NONINTERACTIVE", "1")
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("could not run bash: {e}"))?;
    if out.status.success() {
        return Ok(());
    }
    let log = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    let tail: Vec<&str> = log.lines().filter(|l| !l.trim().is_empty()).collect();
    let tail = tail[tail.len().saturating_sub(3)..].join("; ");
    Err(if tail.is_empty() { format!("installer exited with {}", out.status) } else { tail })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_tag_from_the_redirect() {
        let h = "HTTP/2 302\r\nserver: github.com\r\nLocation: https://github.com/sarafanovn/sfxc/releases/tag/v0.2.0\r\n\r\n";
        assert_eq!(tag_from_headers(h).as_deref(), Some("0.2.0"));
        let bare = "location: https://github.com/BurntSushi/ripgrep/releases/tag/15.2.0\n";
        assert_eq!(tag_from_headers(bare).as_deref(), Some("15.2.0"));
    }

    #[test]
    fn no_release_redirects_to_the_list() {
        assert_eq!(tag_from_headers("HTTP/2 302\r\nlocation: https://github.com/sarafanovn/sfxc/releases\r\n"), None);
        assert_eq!(tag_from_headers("HTTP/2 200\r\n"), None);
    }

    #[test]
    fn compares_versions_numerically() {
        assert!(is_newer("0.10.0", "0.9.9"));
        assert!(is_newer("1.0", "0.9.0"));
        assert!(is_newer("0.2.0", "0.1.0-beta"));
        assert!(!is_newer("0.1.0", "0.1.0"));
        assert!(!is_newer("0.1", "0.1.0"));
        assert!(!is_newer("0.0.9", "0.1.0"));
    }

    #[test]
    fn fresh_cache_skips_the_network() {
        let cache = std::env::temp_dir().join(format!("sfxc-update-{}", std::process::id()));
        write_cache(&cache, 1_000, Some("99.0.0"));
        assert!(cache_is_fresh(&cache, 1_000 + 60));
        assert_eq!(check(&cache, 1_000 + 60, Duration::from_secs(1)).as_deref(), Some("99.0.0"));
        write_cache(&cache, 1_000, None);
        assert_eq!(check(&cache, 1_000 + 60, Duration::from_secs(1)), None);
        assert!(!cache_is_fresh(&cache, 1_000 + CHECK_EVERY.as_secs()));
        let _ = std::fs::remove_file(&cache);
    }
}

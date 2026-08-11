//! Idempotent remote.it agent bootstrap for fleet Pis missing the install-time agent.
//!
//! Detection probes match the remote.it device package (connectd, schannel, remoteit
//! binary). Install uses the same one-liner as standard Pi provisioning.

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::process::Command;
use tokio::time::{timeout, Duration as TokioDuration};

const DEFAULT_REGISTRATION_CODE: &str = "42ACAB78-4E27-5E6E-9D6F-016FEE9B0E04";
const INSTALL_SCRIPT_URL: &str = "https://downloads.remote.it/remoteit/install_agent.sh";
const INSTALL_TIMEOUT_SECS: u64 = 600;
const RETRY_AFTER_HOURS: i64 = 24;

/// remote.it device package units (see remote.it support docs).
const SYSTEMD_UNITS: &[&str] = &["connectd", "schannel"];

static BOOT_ENSURE_STARTED: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Serialize, Deserialize, Default)]
struct RemoteItState {
    last_attempt: Option<DateTime<Utc>>,
    installed: bool,
}

fn state_file_path() -> Option<String> {
    let home = std::env::var("HOME").ok()?;
    Some(format!("{home}/.config/signage/remoteit.state"))
}

async fn command_succeeds(program: &str, args: &[&str]) -> bool {
    match Command::new(program).args(args).output().await {
        Ok(output) => output.status.success(),
        Err(_) => false,
    }
}

async fn systemd_unit_active(unit: &str) -> bool {
    command_succeeds("systemctl", &["is-active", "--quiet", unit]).await
}

/// True when the remote.it device package appears installed and running.
pub async fn is_remoteit_installed() -> bool {
    if Path::new("/usr/bin/remoteit").exists() {
        return true;
    }

    for unit in SYSTEMD_UNITS {
        if systemd_unit_active(unit).await {
            return true;
        }
    }

    command_succeeds("dpkg-query", &["-W", "-f=${Status}", "remoteit"]).await
}

fn registration_code() -> String {
    std::env::var("R3_REGISTRATION_CODE").unwrap_or_else(|_| DEFAULT_REGISTRATION_CODE.to_string())
}

async fn load_state() -> RemoteItState {
    let Some(path) = state_file_path() else {
        return RemoteItState::default();
    };

    match tokio::fs::read_to_string(&path).await {
        Ok(contents) => serde_json::from_str(&contents).unwrap_or_default(),
        Err(_) => RemoteItState::default(),
    }
}

async fn save_state(state: &RemoteItState) {
    let Some(path) = state_file_path() else {
        return;
    };

    if let Some(parent) = Path::new(&path).parent() {
        let _ = tokio::fs::create_dir_all(parent).await;
    }

    if let Ok(json) = serde_json::to_string_pretty(state) {
        let _ = tokio::fs::write(&path, json).await;
    }
}

fn should_retry(state: &RemoteItState) -> bool {
    if state.installed {
        return false;
    }

    let Some(last) = state.last_attempt else {
        return true;
    };

    Utc::now() - last > Duration::hours(RETRY_AFTER_HOURS)
}

async fn run_install_script() -> Result<(), String> {
    let code = registration_code();
    let script = format!(
        "R3_REGISTRATION_CODE=\"{code}\" sh -c \"$(curl -fsSL {INSTALL_SCRIPT_URL})\""
    );

    let output = timeout(
        TokioDuration::from_secs(INSTALL_TIMEOUT_SECS),
        Command::new("sh").arg("-c").arg(&script).output(),
    )
    .await
    .map_err(|_| "remote.it install timed out after 10 minutes".to_string())?
    .map_err(|e| format!("remote.it install failed to execute: {e}"))?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        Err(format!(
            "remote.it install exited with {}: {stderr}{stdout}",
            output.status
        ))
    }
}

async fn ensure_services_started() {
    for unit in SYSTEMD_UNITS {
        let _ = Command::new("systemctl")
            .args(["enable", "--now", unit])
            .output()
            .await;
    }
}

/// Best-effort install; logs errors and never panics. Safe to call from a background task.
pub async fn ensure_remoteit_installed() {
    if is_remoteit_installed().await {
        println!("remote.it: agent already present, skipping install");
        let mut state = load_state().await;
        state.installed = true;
        save_state(&state).await;
        return;
    }

    let state = load_state().await;
    if !should_retry(&state) {
        println!("remote.it: install deferred until retry window elapses");
        return;
    }

    println!("remote.it: agent not found, running install script");

    let mut state = state;
    state.last_attempt = Some(Utc::now());
    state.installed = false;
    save_state(&state).await;

    match run_install_script().await {
        Ok(()) => {
            ensure_services_started().await;
            if is_remoteit_installed().await {
                println!("remote.it: install succeeded");
                state.installed = true;
                save_state(&state).await;
            } else {
                eprintln!(
                    "remote.it: install script finished but agent still not detected; will retry later"
                );
            }
        }
        Err(e) => {
            eprintln!("remote.it: install failed (signage continues): {e}");
        }
    }
}

/// Spawn at most one ensure task per process (once per boot / service restart).
pub fn spawn_ensure_once() {
    if BOOT_ENSURE_STARTED
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return;
    }

    tokio::spawn(async {
        ensure_remoteit_installed().await;
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_retry_when_never_attempted() {
        assert!(should_retry(&RemoteItState::default()));
    }

    #[test]
    fn should_not_retry_when_installed() {
        let state = RemoteItState {
            installed: true,
            last_attempt: Some(Utc::now()),
        };
        assert!(!should_retry(&state));
    }

    #[test]
    fn should_not_retry_within_window() {
        let state = RemoteItState {
            installed: false,
            last_attempt: Some(Utc::now()),
        };
        assert!(!should_retry(&state));
    }

    #[test]
    fn should_retry_after_window() {
        let state = RemoteItState {
            installed: false,
            last_attempt: Some(Utc::now() - Duration::hours(RETRY_AFTER_HOURS + 1)),
        };
        assert!(should_retry(&state));
    }
}

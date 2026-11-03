//! Where `signaged-util` keeps its files, resolved from the XDG base directories.
//!
//! The binary used to assume a login user's home (`$HOME/.config`, `$HOME/.local/share`) and a `pi`
//! user for screenshots. On the OMNIPLAY Pi OS image it runs as the unprivileged `omniplay` system
//! user whose unit sets `XDG_CONFIG_HOME` / `XDG_DATA_HOME` under `/var/lib/omniplay`, the only
//! writable path under `ProtectSystem=strict`. Resolution follows the XDG Base Directory spec: a set,
//! non-empty, absolute `XDG_*_HOME` wins; otherwise the `$HOME` default; a relative XDG value is
//! ignored as the spec says. Fielded devices run as root with no XDG variables and keep
//! `/root/.config/signage` and `/root/.local/share/signage`.
//!
//! This module mirrors `src/paths.rs` of omnicommander-oc1/omniplay-device (the `signaged` daemon)
//! so both binaries read the same `signage.json` and share one data directory.
//!
//! Config: `<config dir>/signage.json`, `<config dir>/remoteit.state`. Data (media cache,
//! `data.json`, `playlist.txt`, screenshots): `<data dir>/…`.

use std::env;
use std::fmt;

/// Sub-directory of each XDG base directory that belongs to the signage binaries.
pub const APP_DIR: &str = "signage";
pub const CONFIG_FILE: &str = "signage.json";
pub const DATA_FILE: &str = "data.json";
pub const PLAYLIST_FILE: &str = "playlist.txt";
/// Sub-directory of the data dir for screenshots: `cleanup_directory` prunes files, not directories,
/// so a capture in progress can never be mistaken for stale media.
pub const SCREENSHOT_DIR: &str = "screenshots";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathError {
    xdg_var: &'static str,
}

impl fmt::Display for PathError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "cannot locate the {} directory: neither {} nor HOME is set to an absolute path",
            APP_DIR, self.xdg_var
        )
    }
}

impl std::error::Error for PathError {}

/// `<XDG base>/signage` where the base is `xdg` when it is an absolute path, else `<home>/<default>`.
fn resolve(xdg: Option<&str>, home: Option<&str>, default_under_home: &str) -> Option<String> {
    let base = match xdg.map(str::trim).filter(|v| v.starts_with('/')) {
        Some(dir) => dir.trim_end_matches('/').to_string(),
        None => {
            let home = home.map(str::trim).filter(|h| h.starts_with('/'))?;
            format!("{}/{default_under_home}", home.trim_end_matches('/'))
        }
    };
    Some(format!("{base}/{APP_DIR}"))
}

/// Config directory from the given `XDG_CONFIG_HOME` / `HOME` values (`None` = unset).
pub fn resolve_config_dir(xdg_config_home: Option<&str>, home: Option<&str>) -> Option<String> {
    resolve(xdg_config_home, home, ".config")
}

/// Data directory from the given `XDG_DATA_HOME` / `HOME` values (`None` = unset).
pub fn resolve_data_dir(xdg_data_home: Option<&str>, home: Option<&str>) -> Option<String> {
    resolve(xdg_data_home, home, ".local/share")
}

/// `$XDG_CONFIG_HOME/signage`, else `$HOME/.config/signage`.
pub fn config_dir() -> Result<String, PathError> {
    let xdg = env::var("XDG_CONFIG_HOME").ok();
    let home = env::var("HOME").ok();
    resolve_config_dir(xdg.as_deref(), home.as_deref()).ok_or(PathError {
        xdg_var: "XDG_CONFIG_HOME",
    })
}

/// `$XDG_DATA_HOME/signage`, else `$HOME/.local/share/signage`.
pub fn data_dir() -> Result<String, PathError> {
    let xdg = env::var("XDG_DATA_HOME").ok();
    let home = env::var("HOME").ok();
    resolve_data_dir(xdg.as_deref(), home.as_deref()).ok_or(PathError {
        xdg_var: "XDG_DATA_HOME",
    })
}

/// `<data dir>/screenshots`, where captures are written before upload.
pub fn screenshot_dir(data_dir: &str) -> String {
    format!("{}/{SCREENSHOT_DIR}", data_dir.trim_end_matches('/'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_image_unit_resolves_under_var_lib_omniplay() {
        // omniplay-pi-os signaged-util.service: HOME=/var/lib/omniplay plus explicit XDG_* under it.
        let home = Some("/var/lib/omniplay");
        assert_eq!(
            resolve_config_dir(Some("/var/lib/omniplay/.config"), home).as_deref(),
            Some("/var/lib/omniplay/.config/signage")
        );
        assert_eq!(
            resolve_data_dir(Some("/var/lib/omniplay/.local/share"), home).as_deref(),
            Some("/var/lib/omniplay/.local/share/signage")
        );
    }

    #[test]
    fn without_xdg_variables_the_legacy_home_layout_is_kept() {
        // Fielded devices: root, HOME=/root, no XDG variables in the old unit.
        assert_eq!(
            resolve_config_dir(None, Some("/root")).as_deref(),
            Some("/root/.config/signage")
        );
        assert_eq!(
            resolve_data_dir(None, Some("/root")).as_deref(),
            Some("/root/.local/share/signage")
        );
        assert_eq!(
            resolve_config_dir(None, Some("/home/pi")).as_deref(),
            Some("/home/pi/.config/signage")
        );
    }

    #[test]
    fn empty_or_relative_xdg_values_are_ignored_as_the_spec_requires() {
        assert_eq!(
            resolve_config_dir(Some(""), Some("/home/pi")).as_deref(),
            Some("/home/pi/.config/signage")
        );
        assert_eq!(
            resolve_data_dir(Some("relative/dir"), Some("/home/pi")).as_deref(),
            Some("/home/pi/.local/share/signage")
        );
        assert_eq!(
            resolve_data_dir(Some("  "), Some("/home/pi")).as_deref(),
            Some("/home/pi/.local/share/signage")
        );
    }

    #[test]
    fn trailing_slashes_do_not_double_up() {
        assert_eq!(
            resolve_config_dir(Some("/var/lib/omniplay/.config/"), None).as_deref(),
            Some("/var/lib/omniplay/.config/signage")
        );
        assert_eq!(
            resolve_data_dir(None, Some("/var/lib/omniplay/")).as_deref(),
            Some("/var/lib/omniplay/.local/share/signage")
        );
    }

    #[test]
    fn a_literal_tilde_or_nothing_at_all_is_an_error_not_a_path() {
        // A systemd unit never expands `~`; it must not silently become a relative directory.
        assert_eq!(resolve_config_dir(Some("~/.config"), Some("~")), None);
        assert_eq!(resolve_data_dir(None, None), None);
        assert_eq!(resolve_data_dir(Some(""), Some("")), None);
    }

    #[test]
    fn xdg_wins_over_home_when_both_are_set() {
        assert_eq!(
            resolve_data_dir(Some("/srv/omniplay/data"), Some("/home/pi")).as_deref(),
            Some("/srv/omniplay/data/signage")
        );
    }

    #[test]
    fn screenshots_live_in_their_own_sub_directory_of_the_data_dir() {
        assert_eq!(
            screenshot_dir("/var/lib/omniplay/.local/share/signage"),
            "/var/lib/omniplay/.local/share/signage/screenshots"
        );
        assert_eq!(
            screenshot_dir("/root/.local/share/signage/"),
            "/root/.local/share/signage/screenshots"
        );
    }

    #[test]
    fn path_error_names_the_variable_that_was_missing() {
        let err = PathError {
            xdg_var: "XDG_DATA_HOME",
        };
        assert!(err.to_string().contains("XDG_DATA_HOME"));
        assert!(err.to_string().contains("HOME"));
    }
}

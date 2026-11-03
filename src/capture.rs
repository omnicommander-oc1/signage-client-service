//! Wayland screenshot capture with `grim`, built so the PNG is always written by this process.
//!
//! `grim` connects to the compositor's socket, which rejects any other user. On a fielded device
//! the binary runs as root while the compositor (wayfire/labwc) runs as `pi`, so `grim` has to be
//! run as `pi` through `runuser`; on the OMNIPLAY Pi OS image both are `omniplay` and `grim` runs
//! directly. In either case `grim -` streams the PNG to standard output and the daemon writes the
//! file under its own identity into the data dir (`/root/...` on a fielded device, which `pi`
//! could never write into; `/var/lib/omniplay/...` on the image).

use crate::privilege::{CommandLine, Privilege};
use std::error::Error;
use std::process::Output;

/// The desktop user that owns the Wayland session on a fielded (pre-image) device.
pub const LEGACY_SESSION_USER: &str = "pi";

/// `grim -` against the given socket, as the session user.
///
/// * Root (fielded device): `runuser -u pi -- /usr/bin/env XDG_RUNTIME_DIR=… WAYLAND_DISPLAY=… grim -`;
///   `runuser` strips the environment, so the variables go through `env`.
/// * Unprivileged (image): `grim -` with the two variables set on the child.
pub fn grim_command(privilege: Privilege, runtime_dir: &str, wayland_display: &str) -> CommandLine {
    let runtime_dir_var = format!("XDG_RUNTIME_DIR={runtime_dir}");
    let wayland_display_var = format!("WAYLAND_DISPLAY={wayland_display}");
    match privilege {
        Privilege::Root => CommandLine::new(
            "runuser",
            &[
                "-u",
                LEGACY_SESSION_USER,
                "--",
                "/usr/bin/env",
                &runtime_dir_var,
                &wayland_display_var,
                "grim",
                "-",
            ],
        ),
        Privilege::Unprivileged => CommandLine::new("grim", &["-"])
            .with_env("XDG_RUNTIME_DIR", runtime_dir)
            .with_env("WAYLAND_DISPLAY", wayland_display),
    }
}

/// The PNG from a finished `grim -`, or `None` when grim failed or produced nothing.
pub fn png_from_output(output: &Output) -> Option<&[u8]> {
    if output.status.success() && !output.stdout.is_empty() {
        Some(&output.stdout)
    } else {
        None
    }
}

/// Writes `png` to `out_path` as this process. `None` writes nothing and returns `Ok(false)`, so a
/// failed capture never leaves an empty file behind.
pub async fn save_png(png: Option<&[u8]>, out_path: &str) -> Result<bool, Box<dyn Error>> {
    match png {
        Some(bytes) => {
            tokio::fs::write(out_path, bytes).await?;
            Ok(true)
        }
        None => Ok(false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::process::ExitStatusExt;
    use std::process::ExitStatus;

    fn words(command: &CommandLine) -> Vec<&str> {
        let mut words = vec![command.program.as_str()];
        words.extend(command.args.iter().map(String::as_str));
        words
    }

    fn output(code: i32, stdout: &[u8]) -> Output {
        Output {
            status: ExitStatus::from_raw(code << 8),
            stdout: stdout.to_vec(),
            stderr: Vec::new(),
        }
    }

    #[test]
    fn as_root_grim_runs_as_pi_through_runuser_and_env_and_writes_to_stdout() {
        let command = grim_command(Privilege::Root, "/run/user/1000", "wayland-1");
        assert_eq!(
            words(&command),
            [
                "runuser",
                "-u",
                "pi",
                "--",
                "/usr/bin/env",
                "XDG_RUNTIME_DIR=/run/user/1000",
                "WAYLAND_DISPLAY=wayland-1",
                "grim",
                "-"
            ]
        );
        // runuser strips the environment; nothing may rely on variables set on the child.
        assert!(command.env.is_empty());
    }

    #[test]
    fn unprivileged_grim_runs_directly_with_the_socket_in_its_environment() {
        let command = grim_command(Privilege::Unprivileged, "/run/user/1000", "wayland-0");
        assert_eq!(words(&command), ["grim", "-"]);
        assert_eq!(
            command.env,
            [
                ("XDG_RUNTIME_DIR".to_string(), "/run/user/1000".to_string()),
                ("WAYLAND_DISPLAY".to_string(), "wayland-0".to_string()),
            ]
        );
        assert!(!words(&command).contains(&"runuser"));
    }

    #[test]
    fn neither_branch_hands_grim_a_path_to_write() {
        // The PNG must come back on stdout so the daemon writes it under its own identity.
        for privilege in [Privilege::Root, Privilege::Unprivileged] {
            let command = grim_command(privilege, "/run/user/1000", "wayland-0");
            assert_eq!(command.args.last().map(String::as_str), Some("-"));
            assert!(!command.args.iter().any(|a| a.ends_with(".png")));
        }
    }

    #[test]
    fn a_successful_grim_yields_its_stdout_as_the_png() {
        let out = output(0, b"\x89PNG\r\n\x1a\n...");
        assert_eq!(png_from_output(&out), Some(&b"\x89PNG\r\n\x1a\n..."[..]));
    }

    #[test]
    fn a_failed_or_silent_grim_yields_nothing() {
        assert_eq!(png_from_output(&output(1, b"\x89PNG")), None);
        assert_eq!(png_from_output(&output(0, b"")), None);
        assert_eq!(png_from_output(&output(127, b"")), None);
    }

    #[tokio::test]
    async fn a_failed_capture_writes_no_file_and_reports_false() {
        let dir =
            std::env::temp_dir().join(format!("signaged-util-capture-{}", uuid::Uuid::new_v4()));
        tokio::fs::create_dir_all(&dir).await.unwrap();
        let out_path = dir.join("screenshot_temp.png");
        let out_path = out_path.to_str().unwrap();

        let failed = output(1, b"");
        assert!(!save_png(png_from_output(&failed), out_path).await.unwrap());
        let silent = output(0, b"");
        assert!(!save_png(png_from_output(&silent), out_path).await.unwrap());
        assert!(!std::path::Path::new(out_path).exists());

        let ok = output(0, b"\x89PNG");
        assert!(save_png(png_from_output(&ok), out_path).await.unwrap());
        assert_eq!(tokio::fs::read(out_path).await.unwrap(), b"\x89PNG");

        tokio::fs::remove_dir_all(&dir).await.unwrap();
    }
}

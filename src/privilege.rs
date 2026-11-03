//! How `signaged-util` restarts the playback service and reboots the device, which depends on the
//! user it runs as.
//!
//! * Legacy fleet: the old unit runs the binary as root with no sandbox. It keeps calling
//!   `sudo systemctl restart signaged.service` and `sudo reboot`, exactly as before.
//! * OMNIPLAY Pi OS image: the binary runs as the unprivileged `omniplay` user under
//!   `NoNewPrivileges=yes`, where `sudo` cannot escalate (the kernel ignores the setuid bit) and the
//!   user has no sudo rights anyway. It calls `systemctl restart signaged.service` and
//!   `systemctl reboot` directly: `systemctl` asks systemd (`org.freedesktop.systemd1`) and logind
//!   (`org.freedesktop.login1`) over the system D-Bus, and those authorise the caller through polkit.
//!   Nothing setuid is involved, so it works under `NoNewPrivileges`. The image installs a polkit rule
//!   for the `omniplay` user; a copy lives in this repo at `polkit/10-omniplay-signaged-util.rules`.
//!
//! Plain `systemctl` also works for root, so when the uid cannot be determined the unprivileged
//! form is used: it is the one that works in both cases.

use std::fmt;
use tokio::process::Command;

/// The systemd unit of the playback daemon (omnicommander-oc1/omniplay-device).
pub const SIGNAGED_UNIT: &str = "signaged.service";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Privilege {
    /// uid 0: the legacy fleet unit.
    Root,
    /// Any other uid: the `omniplay` user on the image.
    Unprivileged,
}

impl Privilege {
    pub fn from_uid(uid: u32) -> Self {
        if uid == 0 {
            Privilege::Root
        } else {
            Privilege::Unprivileged
        }
    }
}

/// A program, its arguments and extra environment, kept as data so the choice can be tested
/// without running anything.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandLine {
    pub program: String,
    pub args: Vec<String>,
    /// Variables set on the child on top of the inherited environment.
    pub env: Vec<(String, String)>,
}

impl CommandLine {
    pub fn new(program: &str, args: &[&str]) -> Self {
        CommandLine {
            program: program.to_string(),
            args: args.iter().map(|a| a.to_string()).collect(),
            env: Vec::new(),
        }
    }

    pub fn with_env(mut self, name: &str, value: &str) -> Self {
        self.env.push((name.to_string(), value.to_string()));
        self
    }

    pub fn to_command(&self) -> Command {
        let mut command = Command::new(&self.program);
        command.args(&self.args);
        for (name, value) in &self.env {
            command.env(name, value);
        }
        command
    }
}

impl fmt::Display for CommandLine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (name, value) in &self.env {
            write!(f, "{name}={value} ")?;
        }
        write!(f, "{}", self.program)?;
        for arg in &self.args {
            write!(f, " {arg}")?;
        }
        Ok(())
    }
}

/// `systemctl restart <unit>`, through `sudo` only when running as root (legacy fleet).
pub fn restart_unit_command(privilege: Privilege, unit: &str) -> CommandLine {
    match privilege {
        Privilege::Root => CommandLine::new("sudo", &["systemctl", "restart", unit]),
        Privilege::Unprivileged => CommandLine::new("systemctl", &["restart", unit]),
    }
}

/// Reboot the device: `sudo reboot` as root (legacy fleet), `systemctl reboot` otherwise (logind +
/// polkit on the image).
pub fn reboot_command(privilege: Privilege) -> CommandLine {
    match privilege {
        Privilege::Root => CommandLine::new("sudo", &["reboot"]),
        Privilege::Unprivileged => CommandLine::new("systemctl", &["reboot"]),
    }
}

/// The effective uid from the text of `/proc/self/status` (`Uid:\treal\teffective\tsaved\tfs`).
pub fn effective_uid_from_proc_status(status: &str) -> Option<u32> {
    status
        .lines()
        .find_map(|line| line.strip_prefix("Uid:"))
        .and_then(|rest| rest.split_whitespace().nth(1))
        .and_then(|uid| uid.parse().ok())
}

/// Who this process runs as. Reads `/proc/self/status`; falls back to `id -u`; when neither works,
/// `Unprivileged`, whose commands also work for root.
pub async fn current_privilege() -> Privilege {
    if let Ok(status) = tokio::fs::read_to_string("/proc/self/status").await {
        if let Some(uid) = effective_uid_from_proc_status(&status) {
            return Privilege::from_uid(uid);
        }
    }
    match Command::new("id").arg("-u").output().await {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout)
            .trim()
            .parse()
            .map(Privilege::from_uid)
            .unwrap_or(Privilege::Unprivileged),
        _ => Privilege::Unprivileged,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(command: &CommandLine) -> Vec<&str> {
        let mut words = vec![command.program.as_str()];
        words.extend(command.args.iter().map(String::as_str));
        words
    }

    #[test]
    fn root_is_uid_zero_and_nothing_else() {
        assert_eq!(Privilege::from_uid(0), Privilege::Root);
        assert_eq!(Privilege::from_uid(1000), Privilege::Unprivileged);
        assert_eq!(Privilege::from_uid(65534), Privilege::Unprivileged);
    }

    #[test]
    fn the_legacy_fleet_keeps_restarting_through_sudo() {
        assert_eq!(
            words(&restart_unit_command(Privilege::Root, SIGNAGED_UNIT)),
            ["sudo", "systemctl", "restart", "signaged.service"]
        );
        assert_eq!(words(&reboot_command(Privilege::Root)), ["sudo", "reboot"]);
    }

    #[test]
    fn the_omniplay_user_calls_systemctl_directly_for_polkit_to_authorise() {
        let restart = restart_unit_command(Privilege::Unprivileged, SIGNAGED_UNIT);
        assert_eq!(
            words(&restart),
            ["systemctl", "restart", "signaged.service"]
        );
        let reboot = reboot_command(Privilege::Unprivileged);
        assert_eq!(words(&reboot), ["systemctl", "reboot"]);
        // Under NoNewPrivileges=yes a setuid helper is useless: nothing may go through sudo.
        assert!(restart.program != "sudo" && reboot.program != "sudo");
        assert!(!restart.args.iter().any(|a| a == "sudo"));
    }

    #[test]
    fn command_line_display_is_the_shell_form() {
        assert_eq!(
            reboot_command(Privilege::Unprivileged).to_string(),
            "systemctl reboot"
        );
        assert_eq!(
            restart_unit_command(Privilege::Root, SIGNAGED_UNIT).to_string(),
            "sudo systemctl restart signaged.service"
        );
    }

    #[test]
    fn effective_uid_is_the_second_field_of_the_uid_line() {
        let status = "Name:\tsignaged-util\nUmask:\t0022\nState:\tS (sleeping)\nUid:\t1000\t1000\t1000\t1000\nGid:\t1000\t1000\t1000\t1000\n";
        assert_eq!(effective_uid_from_proc_status(status), Some(1000));
        let root = "Uid:\t0\t0\t0\t0\n";
        assert_eq!(effective_uid_from_proc_status(root), Some(0));
        // setuid binary launched by uid 1000: real 1000, effective 0.
        let setuid = "Uid:\t1000\t0\t0\t0\n";
        assert_eq!(effective_uid_from_proc_status(setuid), Some(0));
    }

    #[test]
    fn a_status_without_a_uid_line_yields_nothing() {
        assert_eq!(effective_uid_from_proc_status(""), None);
        assert_eq!(effective_uid_from_proc_status("Gid:\t0\t0\t0\t0\n"), None);
        assert_eq!(effective_uid_from_proc_status("Uid:\tx\ty\n"), None);
    }

    #[tokio::test]
    async fn current_privilege_matches_the_test_runner_uid() {
        // Whatever user runs the tests, the answer must agree with `id -u`.
        let expected = match Command::new("id").arg("-u").output().await {
            Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout)
                .trim()
                .parse::<u32>()
                .map(Privilege::from_uid)
                .ok(),
            _ => None,
        };
        if let Some(expected) = expected {
            assert_eq!(current_privilege().await, expected);
        }
    }
}

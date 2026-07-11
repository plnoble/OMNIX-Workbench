//! Spawn child processes WITHOUT flashing a console window on Windows.
//!
//! `#![windows_subsystem = "windows"]` hides only OUR OWN console; every child
//! process we spawn (`--version` probes, `npm view`, `nvidia-smi`, `git`,
//! `powershell`, the agent CLIs) still pops its own console window unless we
//! pass `CREATE_NO_WINDOW`. On a machine with several agents installed,
//! launching the app or opening the 智能体 tab fired a burst of these windows
//! and blocked the UI while the blocking `.output()` probes ran. Applying
//! `.no_window()` at every spawn site suppresses them. No-op off Windows.

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Extension implemented for both `std::process::Command` and
/// `tokio::process::Command` so call sites read the same regardless of which
/// flavour they use.
pub trait NoWindow {
    /// Suppress the child process' console window (Windows). No-op elsewhere.
    fn no_window(&mut self) -> &mut Self;
}

impl NoWindow for std::process::Command {
    fn no_window(&mut self) -> &mut Self {
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            self.creation_flags(CREATE_NO_WINDOW);
        }
        self
    }
}

impl NoWindow for tokio::process::Command {
    fn no_window(&mut self) -> &mut Self {
        #[cfg(windows)]
        {
            self.creation_flags(CREATE_NO_WINDOW);
        }
        self
    }
}

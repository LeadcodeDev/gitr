//! Resolves the `PATH` a login shell would export, so a `git` subprocess launched from
//! a macOS GUI app can find credential helpers and other tools the way it would from a
//! terminal.
//!
//! A GUI application on macOS inherits a truncated `PATH` from `launchd`, not the one
//! set up by `~/.zprofile`, `~/.zshrc` or similar. Spawning the user's `$SHELL` as a
//! login shell and capturing what it exports is the same approach Zed and VS Code use.

use std::env;
use std::path::Path;
use std::process::Command;
use std::sync::OnceLock;

#[cfg(test)]
use std::sync::atomic::{AtomicUsize, Ordering};

const PATH_BEGIN_MARKER: &str = "__gitr_login_path_begin__";
const PATH_END_MARKER: &str = "__gitr_login_path_end__";

/// Where a resolved `PATH` came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoginPathSource {
    LoginShell,
    InheritedFallback,
}

/// The result of resolving the login shell's `PATH`, and how it was obtained.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoginPathResolution {
    pub path: String,
    pub source: LoginPathSource,
}

static LOGIN_PATH: OnceLock<LoginPathResolution> = OnceLock::new();

/// The process-wide login `PATH`, resolved on first use and cached for the lifetime of
/// the process. Spawning a login shell is slow enough that doing it per `git`
/// invocation would be a visible latency regression.
pub fn login_path() -> &'static LoginPathResolution {
    LOGIN_PATH.get_or_init(resolve_login_path_from_environment)
}

fn resolve_login_path_from_environment() -> LoginPathResolution {
    #[cfg(test)]
    RESOLUTION_CALLS.fetch_add(1, Ordering::SeqCst);

    let shell = env::var_os("SHELL");
    resolve_login_path(shell.as_deref().map(Path::new))
}

#[cfg(test)]
static RESOLUTION_CALLS: AtomicUsize = AtomicUsize::new(0);

fn resolve_login_path(shell: Option<&Path>) -> LoginPathResolution {
    match shell.and_then(capture_login_path) {
        Some(path) if !path.is_empty() => LoginPathResolution {
            path,
            source: LoginPathSource::LoginShell,
        },
        _ => LoginPathResolution {
            path: env::var("PATH").unwrap_or_default(),
            source: LoginPathSource::InheritedFallback,
        },
    }
}

fn capture_login_path(shell: &Path) -> Option<String> {
    let command = format!(
        "printf '%s' '{PATH_BEGIN_MARKER}'; printf '%s' \"$PATH\"; printf '%s' '{PATH_END_MARKER}'"
    );
    let output = Command::new(shell).args(["-ilc", &command]).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8(output.stdout).ok()?;
    let start = stdout.find(PATH_BEGIN_MARKER)? + PATH_BEGIN_MARKER.len();
    let end = start + stdout[start..].find(PATH_END_MARKER)?;
    Some(stdout[start..end].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn falls_back_when_the_shell_binary_does_not_exist() {
        let bogus = Path::new("/definitely/not/a/real/shell/binary");
        let resolution = resolve_login_path(Some(bogus));
        assert_eq!(resolution.source, LoginPathSource::InheritedFallback);
    }

    #[test]
    fn falls_back_when_no_shell_is_configured() {
        let resolution = resolve_login_path(None);
        assert_eq!(resolution.source, LoginPathSource::InheritedFallback);
    }

    #[test]
    fn fallback_path_matches_the_inherited_environment() {
        let bogus = Path::new("/definitely/not/a/real/shell/binary");
        let resolution = resolve_login_path(Some(bogus));
        assert_eq!(resolution.path, env::var("PATH").unwrap_or_default());
    }

    #[test]
    fn caches_the_resolution_instead_of_repeating_it() {
        let before = RESOLUTION_CALLS.load(Ordering::SeqCst);
        login_path();
        login_path();
        let after = RESOLUTION_CALLS.load(Ordering::SeqCst);
        assert!(after - before <= 1);
    }
}

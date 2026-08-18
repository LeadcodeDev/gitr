//! Saving and restoring window preferences across launches: the dock layout and the
//! theme preference.
//!
//! [`DockAreaState`] and [`ThemePreference`] are both `Serialize`/`Deserialize` and carry
//! no gpui entities, so the round trip through this module is ordinary JSON on disk — no
//! different in kind from any other config file. Dock layout saves are debounced by
//! [`crate::workspace::Workspace`], since `DockEvent::LayoutChanged` fires on every drag
//! frame while a panel is being resized; a theme preference change is a rare, deliberate
//! click, so it saves immediately.
//!
//! A failure here is a logged inconvenience, not a crash: nothing in this module panics,
//! and every fallible function returns a `Result` for the caller to log and move past.

use std::path::{Path, PathBuf};

use gpui_component::dock::DockAreaState;

use crate::theme_preference::ThemePreference;

const APPLICATION_SUPPORT_DIR: &str = "Library/Application Support/gitr";
const DOCK_LAYOUT_FILE: &str = "dock-layout.json";
const THEME_PREFERENCE_FILE: &str = "theme-preference.json";

/// Where the dock layout lives for the signed-in user, or `None` if `$HOME` is unset.
///
/// Not cached: resolving `$HOME` is one `env::var_os` call, cheap enough to redo on every
/// save rather than risk a stale value if the process environment ever changed.
pub fn dock_layout_path() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(
        PathBuf::from(home)
            .join(APPLICATION_SUPPORT_DIR)
            .join(DOCK_LAYOUT_FILE),
    )
}

/// Persists `state` to `path`, creating its parent directory if it does not exist yet.
///
/// Blocking: call this from `cx.background_executor()`, never on the frame thread.
pub fn save_to(path: &Path, state: &DockAreaState) -> anyhow::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let json = serde_json::to_string_pretty(state)?;
    std::fs::write(path, json)?;
    Ok(())
}

/// Reads and parses the dock layout at `path`.
///
/// Blocking, and meant to run once at startup before the first frame — matching how
/// `crates/story/examples/dock.rs` in the gpui-component checkout loads its own layout.
pub fn load_from(path: &Path) -> anyhow::Result<DockAreaState> {
    let json = std::fs::read_to_string(path)?;
    Ok(serde_json::from_str(&json)?)
}

/// Saves `state` to the user's application support directory.
pub fn save(state: &DockAreaState) -> anyhow::Result<()> {
    let path = dock_layout_path().ok_or_else(|| anyhow::anyhow!("$HOME is not set"))?;
    save_to(&path, state)
}

/// Loads the dock layout from the user's application support directory, if one exists
/// and parses cleanly. Any failure — missing file, unreadable JSON, a shape from a much
/// older version — is treated the same way: fall back to the default layout.
pub fn load() -> Option<DockAreaState> {
    load_from(&dock_layout_path()?).ok()
}

/// Where the theme preference lives for the signed-in user, or `None` if `$HOME` is
/// unset. See [`dock_layout_path`] — same directory, same not-cached reasoning.
pub fn theme_preference_path() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(
        PathBuf::from(home)
            .join(APPLICATION_SUPPORT_DIR)
            .join(THEME_PREFERENCE_FILE),
    )
}

/// Persists `preference` to `path`, creating its parent directory if it does not exist
/// yet. Blocking: call this from `cx.background_executor()`, never on the frame thread.
pub fn save_theme_preference_to(path: &Path, preference: &ThemePreference) -> anyhow::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let json = serde_json::to_string_pretty(preference)?;
    std::fs::write(path, json)?;
    Ok(())
}

/// Reads and parses the theme preference at `path`. Blocking, meant to run once at
/// startup before the first frame — same shape as [`load_from`].
pub fn load_theme_preference_from(path: &Path) -> anyhow::Result<ThemePreference> {
    let json = std::fs::read_to_string(path)?;
    Ok(serde_json::from_str(&json)?)
}

/// Saves `preference` to the user's application support directory.
pub fn save_theme_preference(preference: &ThemePreference) -> anyhow::Result<()> {
    let path = theme_preference_path().ok_or_else(|| anyhow::anyhow!("$HOME is not set"))?;
    save_theme_preference_to(&path, preference)
}

/// Loads the theme preference from the user's application support directory, if one
/// exists and parses cleanly. Any failure falls back to `None`, mirroring [`load`].
pub fn load_theme_preference() -> Option<ThemePreference> {
    load_theme_preference_from(&theme_preference_path()?).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Axis, px};
    use gpui_component::dock::{PanelInfo, PanelState};

    fn sample_state() -> DockAreaState {
        DockAreaState {
            version: Some(3),
            center: PanelState {
                panel_name: "Split".to_string(),
                children: vec![
                    PanelState {
                        panel_name: "HistorySeam".to_string(),
                        children: Vec::new(),
                        info: PanelInfo::tabs(0),
                    },
                    PanelState {
                        panel_name: "DetailSeam".to_string(),
                        children: Vec::new(),
                        info: PanelInfo::tabs(0),
                    },
                ],
                info: PanelInfo::stack(vec![px(600.), px(240.)], Axis::Vertical),
            },
            left_dock: None,
            right_dock: None,
            bottom_dock: None,
        }
    }

    fn scratch_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "gitr-persistence-test-{}-{name}",
            std::process::id()
        ))
    }

    #[test]
    fn round_trips_a_nested_layout_through_disk() {
        let path = scratch_path("round-trip.json");
        let state = sample_state();

        save_to(&path, &state).expect("save must succeed against a writable temp path");
        let loaded = load_from(&path).expect("load must succeed right after save");

        assert_eq!(loaded, state);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_missing_file_is_an_error_not_a_panic() {
        let path = scratch_path("does-not-exist.json");
        assert!(load_from(&path).is_err());
    }

    #[test]
    fn saving_creates_missing_parent_directories() {
        let path = scratch_path("nested-dir").join("dock-layout.json");
        let state = sample_state();

        save_to(&path, &state).expect("save must create its own parent directory");
        assert!(path.exists());

        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn round_trips_a_theme_preference_through_disk() {
        let path = scratch_path("theme-round-trip.json");

        save_theme_preference_to(&path, &ThemePreference::Dark)
            .expect("save must succeed against a writable temp path");
        let loaded = load_theme_preference_from(&path).expect("load must succeed right after save");

        assert_eq!(loaded, ThemePreference::Dark);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_missing_theme_preference_file_is_an_error_not_a_panic() {
        let path = scratch_path("theme-does-not-exist.json");
        assert!(load_theme_preference_from(&path).is_err());
    }

    #[test]
    fn a_corrupt_theme_preference_file_is_an_error_not_a_panic() {
        let path = scratch_path("theme-corrupt.json");
        std::fs::write(&path, b"not json").expect("must be able to write the scratch file");

        assert!(load_theme_preference_from(&path).is_err());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_missing_theme_preference_file_falls_back_to_the_default_preference() {
        let path = scratch_path("theme-default-fallback-missing.json");
        let preference = load_theme_preference_from(&path).unwrap_or_default();
        assert_eq!(preference, ThemePreference::default());
    }

    #[test]
    fn a_corrupt_theme_preference_file_falls_back_to_the_default_preference() {
        let path = scratch_path("theme-default-fallback-corrupt.json");
        std::fs::write(&path, b"{ not json").expect("must be able to write the scratch file");

        let preference = load_theme_preference_from(&path).unwrap_or_default();
        assert_eq!(preference, ThemePreference::default());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn saving_a_theme_preference_creates_missing_parent_directories() {
        let path = scratch_path("theme-nested-dir").join("theme-preference.json");

        save_theme_preference_to(&path, &ThemePreference::Light)
            .expect("save must create its own parent directory");
        assert!(path.exists());

        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }
}

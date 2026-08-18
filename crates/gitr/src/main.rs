//! gitr's composition root: resolves the repository to open, then wires it into the
//! window. A path that does not lead to a Git repository is a startup error, reported on
//! stderr with a non-zero exit — never a panic, and never an empty window.

use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

use domain::RepositoryError;
use gpui::{App, AppContext};
use gpui_component::{Root, TitleBar};
use ui::Workspace;

fn main() -> ExitCode {
    let requested = env::args_os().nth(1).map(PathBuf::from);
    let path = match resolve_repository_root(requested) {
        Ok(path) => path,
        Err(error) => {
            eprintln!("gitr: {error}");
            return ExitCode::FAILURE;
        }
    };

    gpui_platform::application()
        .with_assets(gpui_component_assets::Assets)
        .run(move |cx: &mut App| {
            gpui_component::init(cx);
            ui::init(cx);

            let path = path.clone();
            cx.spawn(async move |cx| {
                cx.open_window(TitleBar::window_options(), |window, cx| {
                    let workspace = cx.new(|cx| Workspace::new(path, window, cx));
                    cx.new(|cx| Root::new(workspace, window, cx))
                })
                .expect("gitr cannot run without a window");
            })
            .detach();
        });

    ExitCode::SUCCESS
}

/// Resolves `requested` — the first command-line argument, or the current directory when
/// none was given — to the root of the Git repository it names, by walking up looking for
/// a `.git` entry the way `git` itself does.
///
/// A relative or symlinked path is canonicalised first: `RepositoryError`'s messages name
/// what the user typed, but the walk itself needs an absolute path with no `..` segments
/// to terminate correctly at the filesystem root.
fn resolve_repository_root(requested: Option<PathBuf>) -> Result<PathBuf, RepositoryError> {
    let start = match requested {
        Some(path) => path,
        None => env::current_dir().map_err(|_| RepositoryError::Unreadable(PathBuf::from(".")))?,
    };

    let canonical = start
        .canonicalize()
        .map_err(|_| RepositoryError::Unreadable(start.clone()))?;

    let mut current = canonical.as_path();
    loop {
        if current.join(".git").exists() {
            return Ok(current.to_path_buf());
        }
        current = match current.parent() {
            Some(parent) => parent,
            None => return Err(RepositoryError::NotARepository(start)),
        };
    }
}

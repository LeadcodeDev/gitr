//! gitr's composition root: resolves which projects the window opens on, then wires the
//! active one into the window. A path argument that does not lead to a Git repository is
//! a startup error, reported on stderr with a non-zero exit — never a panic, and never an
//! empty window.
//!
//! `cx.on_action::<Quit>` runs before `Workspace::new` — reached only inside the
//! `cx.spawn` below, on a later turn of the executor — ever gets a chance to call
//! `cx.set_menus`. A menu item's *presence* does not depend on a handler existing for it,
//! so an application menu carrying Quit without this line would still render a working
//! "Quit gitr" item that silently does nothing when clicked, exactly the failure mode
//! `crates/ui/src/actions.rs` warns about.
//!
//! `Workspace::register_menu_actions` is that same registration for the other ten menu
//! actions, and belongs here for the same reason: a menu item is only ever enabled by a
//! global handler, never by one on the window's element tree.

use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

use domain::RepositoryError;
use gpui::{App, AppContext};
use gpui_component::{Root, TitleBar};
use ui::Workspace;
use ui::actions::Quit;
use ui::persistence;
use ui::project::{Project, ProjectList, resolve_repository_root};

fn main() -> ExitCode {
    let requested = env::args_os().nth(1).map(PathBuf::from);

    let mut projects = project_list_at_startup();

    let seed = match requested {
        Some(dir) => Some(dir),
        None if projects.projects.is_empty() => match env::current_dir() {
            Ok(cwd) => Some(cwd),
            Err(_) => {
                eprintln!("gitr: {}", RepositoryError::Unreadable(PathBuf::from(".")));
                return ExitCode::FAILURE;
            }
        },
        None => None,
    };

    if let Some(start) = seed {
        match resolve_repository_root(&start) {
            Ok(root) => projects.add_or_activate(Project::local(root)),
            Err(error) => {
                eprintln!("gitr: {error}");
                return ExitCode::FAILURE;
            }
        }
    }

    if let Err(error) = persistence::save_project_list(&projects) {
        eprintln!("gitr: failed to save project list: {error:#}");
    }

    gpui_platform::application()
        .with_assets(gpui_component_assets::Assets)
        .run(move |cx: &mut App| {
            gpui_component::init(cx);
            ui::init(cx);
            cx.on_action(|_: &Quit, cx| cx.quit());

            let projects = projects.clone();
            cx.spawn(async move |cx| {
                cx.open_window(TitleBar::window_options(), |window, cx| {
                    let workspace = cx.new(|cx| Workspace::new(projects, window, cx));
                    Workspace::register_menu_actions(&workspace, window, cx);
                    cx.new(|cx| Root::new(workspace, window, cx))
                })
                .expect("gitr cannot run without a window");
            })
            .detach();
        });

    ExitCode::SUCCESS
}

/// Reads the persisted project list, logging and starting empty on a genuine failure to
/// parse an existing file. A file that is simply absent — every first launch — is not
/// logged: that is the expected, unremarkable case, exactly as `Workspace` treats a
/// missing dock layout or theme preference file.
fn project_list_at_startup() -> ProjectList {
    let Some(path) = persistence::project_list_path() else {
        return ProjectList::default();
    };
    if !path.exists() {
        return ProjectList::default();
    }
    persistence::load_project_list_from(&path).unwrap_or_else(|error| {
        eprintln!("gitr: failed to read saved project list, starting empty: {error:#}");
        ProjectList::default()
    })
}

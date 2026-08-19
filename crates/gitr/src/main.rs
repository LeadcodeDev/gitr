//! gitr's composition root: resolves which projects the window opens on, then wires the
//! active one into the window.
//!
//! Running `gitr` with no argument opens the repository containing the working directory.
//! That is the command's primary use, so it happens whatever else is already saved — not
//! only on a first launch with an empty list. A working directory outside any repository
//! is fatal only when no saved project can stand in for it; an explicit path argument
//! that is not a repository is always fatal, because it was asked for by name. Every
//! failure is reported on stderr with a non-zero exit — never a panic, and never an empty
//! window.
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
use std::path::{Path, PathBuf};
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

    let outcome = match &requested {
        Some(path) => activate_repository_at(path, &mut projects),
        None => env::current_dir()
            .map_err(|_| RepositoryError::Unreadable(PathBuf::from(".")))
            .and_then(|cwd| activate_repository_at(&cwd, &mut projects)),
    };

    if let Err(error) = outcome
        && aborts_launch(requested.is_some(), projects.projects.len())
    {
        eprintln!("gitr: {error}");
        return ExitCode::FAILURE;
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

/// Whether failing to open the startup repository aborts the launch rather than falling
/// back to the saved project list.
///
/// A path given on the command line is always fatal when it does not resolve: the user
/// named that directory, and quietly opening some other project instead would hide the
/// mistake. A bare `gitr` run from outside any repository is not, so long as something is
/// saved to show — that is the ordinary case of launching the command from a home
/// directory, and it must not be an error.
fn aborts_launch(path_was_requested: bool, saved_projects: usize) -> bool {
    path_was_requested || saved_projects == 0
}

/// Resolves `start` to the root of the repository containing it and makes that project the
/// active one, adding it to the list when it is not already there.
///
/// `start` is a directory *inside* the repository, not necessarily its root:
/// [`resolve_repository_root`] walks upwards until it finds the `.git` entry, so running
/// `gitr` deep inside a checkout opens the whole repository rather than failing.
fn activate_repository_at(start: &Path, projects: &mut ProjectList) -> Result<(), RepositoryError> {
    let root = resolve_repository_root(start)?;
    projects.add_or_activate(Project::local(root));
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use ui::project::ProjectSource;

    fn checkout(root: &Path) {
        fs::create_dir_all(root.join(".git")).expect("fixture must create a .git directory");
    }

    #[test]
    fn a_bare_run_inside_a_checkout_opens_it_even_when_projects_are_already_saved() {
        let temp = tempfile::tempdir().expect("fixture must create a temporary directory");
        let root = temp
            .path()
            .canonicalize()
            .expect("fixture root must resolve");
        checkout(&root);

        let saved = Project::local(PathBuf::from("/repos/elsewhere"));
        let mut projects = ProjectList {
            projects: vec![saved.clone()],
            active: Some(saved.source.clone()),
        };

        activate_repository_at(&root, &mut projects).expect("the checkout must resolve");

        assert_eq!(
            projects.active_project(),
            Some(&Project::local(root)),
            "typing `gitr` in a repository must open that repository; gating this on an \
             empty project list meant the command did nothing once anything was saved"
        );
        assert_eq!(
            projects.projects.len(),
            2,
            "the previously saved project stays in the list, it is only no longer active"
        );
    }

    #[test]
    fn a_run_from_deep_inside_a_checkout_opens_the_repository_root() {
        let temp = tempfile::tempdir().expect("fixture must create a temporary directory");
        let root = temp
            .path()
            .canonicalize()
            .expect("fixture root must resolve");
        checkout(&root);
        let nested = root.join("crates").join("domain").join("src");
        fs::create_dir_all(&nested).expect("fixture must create the nested directory");

        let mut projects = ProjectList::default();
        activate_repository_at(&nested, &mut projects).expect("the checkout must resolve");

        assert_eq!(
            projects.active_project().map(|project| &project.source),
            Some(&ProjectSource::Local(root)),
            "the working directory is a starting point, not the root: `gitr` run in a \
             subdirectory opens the whole repository"
        );
    }

    #[test]
    fn a_directory_outside_any_repository_leaves_the_list_untouched() {
        let temp = tempfile::tempdir().expect("fixture must create a temporary directory");
        let mut projects = ProjectList::default();

        activate_repository_at(temp.path(), &mut projects)
            .expect_err("a directory with no .git anywhere above it is not a repository");
        assert!(
            projects.projects.is_empty(),
            "a failed resolution must add nothing, or a bad path would be persisted"
        );
    }

    #[test]
    fn a_named_path_that_does_not_resolve_always_aborts() {
        assert!(
            aborts_launch(true, 0),
            "nothing to fall back on, and the path was named"
        );
        assert!(
            aborts_launch(true, 3),
            "a named path that does not resolve must abort even with projects saved, or \
             `gitr /typo` silently opens whatever was open last"
        );
    }

    #[test]
    fn a_bare_run_outside_a_repository_aborts_only_with_nothing_saved() {
        assert!(
            aborts_launch(false, 0),
            "no argument, no repository here, nothing saved: there is nothing to show"
        );
        assert!(
            !aborts_launch(false, 2),
            "running `gitr` from a home directory must open the saved projects, not fail"
        );
    }
}

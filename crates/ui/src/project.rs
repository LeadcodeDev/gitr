//! A project the window can have open: where it lives, what to call it, and the
//! remembered list of every project the user has added, across launches.
//!
//! [`ProjectSource::Local`] is the only source this workstream builds. It is a variant of
//! an enum rather than [`Project`] holding a bare [`PathBuf`] so that a later remote
//! source — cloned or fetched over the network — is a new match arm at the handful of
//! places that open a project, never a reshape of [`Project`], [`ProjectList`] or their
//! callers.
//!
//! [`ProjectList`] is plain data: no gpui entity, no I/O. [`crate::persistence`] is what
//! reads and writes it, the same split it already keeps between the dock layout's shape
//! and its own file handling.

use std::path::{Path, PathBuf};

use domain::RepositoryError;
use serde::{Deserialize, Serialize};

/// Where a project's repository lives.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum ProjectSource {
    Local(PathBuf),
}

/// One entry in the remembered project list.
///
/// `name` is stored rather than derived at every use so a future source whose display
/// name cannot be read from a local path — a remote clone URL, say — has somewhere to
/// put one without changing this shape again.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Project {
    pub name: String,
    pub source: ProjectSource,
}

impl Project {
    /// A local project named after its repository root's own directory name.
    pub fn local(path: PathBuf) -> Self {
        let name = display_name(&path);
        Self {
            name,
            source: ProjectSource::Local(path),
        }
    }
}

/// Every project the user has added, and which one is open.
///
/// `active` names a project by its source rather than its position, so it keeps pointing
/// at the right entry regardless of where that entry ends up in `projects`, and survives
/// round-tripping through disk without needing the list's order to be stable.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectList {
    pub projects: Vec<Project>,
    pub active: Option<ProjectSource>,
}

impl ProjectList {
    /// The active project, falling back to the first entry in `projects` when `active` is
    /// absent or names a project that is no longer in the list. That fallback is what
    /// keeps a hand-edited or stale persisted file from leaving the window with no
    /// project to open rather than an error.
    pub fn active_project(&self) -> Option<&Project> {
        self.active
            .as_ref()
            .and_then(|source| {
                self.projects
                    .iter()
                    .find(|project| &project.source == source)
            })
            .or_else(|| self.projects.first())
    }

    /// Adds `project` if no project with the same source is already present, then makes
    /// it active either way. The caller never has to check membership itself before
    /// deciding whether this is an add or a plain switch — opening a path already on the
    /// list activates it instead of duplicating it.
    pub fn add_or_activate(&mut self, project: Project) {
        let is_new = !self
            .projects
            .iter()
            .any(|existing| existing.source == project.source);
        if is_new {
            self.projects.push(project.clone());
        }
        self.active = Some(project.source);
    }

    /// Makes `source` active if it names a project already in the list, and returns that
    /// project. Does nothing, and returns `None`, for a source this list has never seen —
    /// the caller is expected to pass a source drawn from this same list, never one
    /// constructed on the spot.
    pub fn activate(&mut self, source: &ProjectSource) -> Option<&Project> {
        let index = self
            .projects
            .iter()
            .position(|project| &project.source == source)?;
        self.active = Some(source.clone());
        self.projects.get(index)
    }
}

/// The name a project's title bar entry and switcher row show, for a path this crate can
/// name without opening the repository it leads to: the last path segment.
pub fn display_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

/// Resolves `start` to the root of the Git repository it names, by walking up looking for
/// a `.git` entry the way `git` itself does.
///
/// `start` is canonicalised first: a relative or symlinked path needs an absolute path
/// with no `..` segments for the walk to terminate correctly at the filesystem root.
/// [`RepositoryError`]'s messages are built from `start` as given, not the canonical
/// form, so they name what the user typed or chose rather than its resolved form.
pub fn resolve_repository_root(start: &Path) -> Result<PathBuf, RepositoryError> {
    let canonical = start
        .canonicalize()
        .map_err(|_| RepositoryError::Unreadable(start.to_path_buf()))?;

    let mut current = canonical.as_path();
    loop {
        if current.join(".git").exists() {
            return Ok(current.to_path_buf());
        }
        current = match current.parent() {
            Some(parent) => parent,
            None => return Err(RepositoryError::NotARepository(start.to_path_buf())),
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(name: &str) -> Project {
        Project::local(PathBuf::from(format!("/repos/{name}")))
    }

    #[test]
    fn adding_to_an_empty_list_adds_and_activates() {
        let mut list = ProjectList::default();
        list.add_or_activate(project("a"));

        assert_eq!(list.projects.len(), 1);
        assert_eq!(list.active, Some(project("a").source));
    }

    #[test]
    fn adding_an_already_listed_source_activates_without_duplicating() {
        let mut list = ProjectList::default();
        list.add_or_activate(project("a"));
        list.add_or_activate(project("b"));
        list.add_or_activate(project("a"));

        assert_eq!(list.projects.len(), 2);
        assert_eq!(list.active, Some(project("a").source));
    }

    #[test]
    fn activating_a_known_source_returns_it_and_updates_active() {
        let mut list = ProjectList::default();
        list.add_or_activate(project("a"));
        list.add_or_activate(project("b"));

        let activated = list.activate(&project("a").source);

        assert_eq!(activated, Some(&project("a")));
        assert_eq!(list.active, Some(project("a").source));
    }

    #[test]
    fn activating_an_unknown_source_changes_nothing() {
        let mut list = ProjectList::default();
        list.add_or_activate(project("a"));

        let activated = list.activate(&project("unknown").source);

        assert_eq!(activated, None);
        assert_eq!(list.active, Some(project("a").source));
    }

    #[test]
    fn the_active_project_is_none_for_an_empty_list() {
        assert_eq!(ProjectList::default().active_project(), None);
    }

    #[test]
    fn a_missing_active_pointer_falls_back_to_the_first_project() {
        let list = ProjectList {
            projects: vec![project("a"), project("b")],
            active: None,
        };
        assert_eq!(list.active_project(), Some(&project("a")));
    }

    #[test]
    fn a_stale_active_pointer_falls_back_to_the_first_project() {
        let list = ProjectList {
            projects: vec![project("a"), project("b")],
            active: Some(project("gone").source),
        };
        assert_eq!(list.active_project(), Some(&project("a")));
    }

    #[test]
    fn a_valid_active_pointer_is_used_as_is() {
        let list = ProjectList {
            projects: vec![project("a"), project("b")],
            active: Some(project("b").source),
        };
        assert_eq!(list.active_project(), Some(&project("b")));
    }

    #[test]
    fn a_project_list_round_trips_through_json() {
        let list = ProjectList {
            projects: vec![project("a"), project("b")],
            active: Some(project("b").source),
        };
        let json = serde_json::to_string(&list).expect("list must serialise");
        let restored: ProjectList = serde_json::from_str(&json).expect("list must deserialise");
        assert_eq!(restored, list);
    }

    #[test]
    fn an_empty_project_list_round_trips_through_json() {
        let list = ProjectList::default();
        let json = serde_json::to_string(&list).expect("list must serialise");
        let restored: ProjectList = serde_json::from_str(&json).expect("list must deserialise");
        assert_eq!(restored, list);
    }

    #[test]
    fn resolves_a_repository_roots_own_directory() {
        let temp = tempfile::tempdir().expect("must be able to create a temp dir");
        std::fs::create_dir(temp.path().join(".git")).expect("must be able to create .git");

        let resolved = resolve_repository_root(temp.path()).expect("root must resolve");
        assert_eq!(resolved, temp.path().canonicalize().unwrap());
    }

    #[test]
    fn resolves_a_subdirectory_of_a_repository_to_its_root() {
        let temp = tempfile::tempdir().expect("must be able to create a temp dir");
        std::fs::create_dir(temp.path().join(".git")).expect("must be able to create .git");
        let nested = temp.path().join("src").join("nested");
        std::fs::create_dir_all(&nested).expect("must be able to create a nested directory");

        let resolved = resolve_repository_root(&nested).expect("root must resolve");
        assert_eq!(resolved, temp.path().canonicalize().unwrap());
    }

    #[test]
    fn a_path_with_no_repository_above_it_is_reported_not_a_repository() {
        let temp = tempfile::tempdir().expect("must be able to create a temp dir");

        let error = resolve_repository_root(temp.path()).expect_err("must not resolve");
        assert!(matches!(error, RepositoryError::NotARepository(_)));
    }

    #[test]
    fn a_path_that_does_not_exist_is_reported_unreadable() {
        let temp = tempfile::tempdir().expect("must be able to create a temp dir");
        let missing = temp.path().join("does-not-exist");

        let error = resolve_repository_root(&missing).expect_err("must not resolve");
        assert!(matches!(error, RepositoryError::Unreadable(_)));
    }
}

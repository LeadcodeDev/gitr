//! A project the window can have open: where it lives, what to call it, and the
//! remembered list of every project the user has added, across launches.
//!
//! [`ProjectSource::Local`] opens a repository already on disk. [`ProjectSource::Remote`]
//! is a bare partial clone gitr made itself, in its own cache directory — see
//! [`remote_cache_dir`] for why two different URLs never land in the same place. Both are
//! variants of one enum rather than [`Project`] holding a bare [`PathBuf`] so that opening
//! either kind is a match arm at the handful of places that open a project, never two
//! unrelated code paths.
//!
//! [`ProjectList`] is plain data: no gpui entity, no I/O. [`crate::persistence`] is what
//! reads and writes it, the same split it already keeps between the dock layout's shape
//! and its own file handling.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use domain::RepositoryError;
use serde::{Deserialize, Serialize};

/// A bare partial clone gitr made of a public repository URL.
///
/// `last_synchronised` is `None` until the first [`crate::workspace::Workspace`]-driven
/// fetch completes — a project added but never synchronised must read as "never", not as
/// a silently missing timestamp.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RemoteProject {
    pub url: String,
    pub cache_dir: PathBuf,
    pub last_synchronised: Option<SystemTime>,
}

/// Where a project's repository lives.
///
/// `PartialEq` is written by hand rather than derived: [`RemoteProject::last_synchronised`]
/// changes every time [`crate::workspace::Workspace`] synchronises a project, and
/// [`ProjectList`] identifies a project by comparing sources — a derived `PartialEq` would
/// make a just-synchronised project stop matching the very [`ProjectList::active`] pointer
/// that was recorded for it before the fetch, and [`ProjectList::active_project`] would
/// silently fall back to the first project in the list instead. Two [`Remote`](Self::Remote)
/// sources are the same project when they clone the same URL into the same cache
/// directory; when they were last synchronised is not part of that identity.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum ProjectSource {
    Local(PathBuf),
    Remote(RemoteProject),
}

impl PartialEq for ProjectSource {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (ProjectSource::Local(a), ProjectSource::Local(b)) => a == b,
            (ProjectSource::Remote(a), ProjectSource::Remote(b)) => {
                a.url == b.url && a.cache_dir == b.cache_dir
            }
            _ => false,
        }
    }
}

impl Eq for ProjectSource {}

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

    /// A remote project named after the last path segment of `url`, cloned into
    /// `cache_dir`, never yet synchronised.
    pub fn remote(url: String, cache_dir: PathBuf) -> Self {
        let name = remote_display_name(&url);
        Self {
            name,
            source: ProjectSource::Remote(RemoteProject {
                url,
                cache_dir,
                last_synchronised: None,
            }),
        }
    }
}

/// The number of projects [`crate::sidebar::selector`] shows before the rest need
/// scrolling to reach. Filtering (see [`filter_projects`]) always runs over the whole
/// list, never just this many — the cap is a visible-height limit on the list's
/// container, not a truncation of what search can find.
pub const MAX_VISIBLE_PROJECTS: usize = 5;

/// Every project in `projects` whose name contains `query`, case-insensitively.
///
/// Runs over the full list rather than any already-visible slice of it, which is what
/// lets [`crate::sidebar::selector`] cap how many rows are visible without capping how
/// many a search can reach. An empty or all-whitespace `query` matches everything.
pub fn filter_projects<'a>(projects: &'a [Project], query: &str) -> Vec<&'a Project> {
    let needle = query.trim().to_lowercase();
    if needle.is_empty() {
        return projects.iter().collect();
    }
    projects
        .iter()
        .filter(|project| project.name.to_lowercase().contains(&needle))
        .collect()
}

/// The name a remote project's row and title bar show: the URL's last path segment with
/// a trailing `.git` stripped, or the whole trimmed URL if that segment is empty (a bare
/// `https://host` with no path, which [`validate_remote_url`] otherwise lets through).
fn remote_display_name(url: &str) -> String {
    let trimmed = url.trim().trim_end_matches('/');
    let candidate = trimmed.rsplit('/').next().unwrap_or(trimmed);
    let candidate = candidate.strip_suffix(".git").unwrap_or(candidate);
    if candidate.is_empty() {
        trimmed.to_string()
    } else {
        candidate.to_string()
    }
}

const FNV_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// FNV-1a over `bytes`, chosen over `std::collections::hash_map::DefaultHasher` because
/// that hasher's algorithm is explicitly unspecified across releases — unsuitable for a
/// value this module writes to disk and must keep able to derive consistently within a
/// single running process. FNV-1a needs no dependency and is fully pinned by this
/// function's own source.
fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = FNV_OFFSET_BASIS;
    for &byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

const REMOTE_CACHE_SLUG_MAX_LEN: usize = 48;

/// A filesystem-safe, human-readable prefix for `url`: its scheme stripped, every
/// character outside `[A-Za-z0-9._-]` collapsed to a single `-`, capped in length so a
/// very long URL cannot produce an unwieldy directory name.
fn sanitize_for_filesystem(url: &str) -> String {
    let stripped = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .unwrap_or(url);

    let mut slug = String::new();
    for ch in stripped.chars() {
        if slug.chars().count() >= REMOTE_CACHE_SLUG_MAX_LEN {
            break;
        }
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == '.' {
            slug.push(ch);
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
    }

    let slug = slug.trim_matches('-');
    if slug.is_empty() {
        "remote".to_string()
    } else {
        slug.to_string()
    }
}

/// The cache directory a clone of `url` lands in under `root`.
///
/// The directory name is `<sanitized-url>-<fnv1a64(url) as 16 lowercase hex digits>`.
/// Two different URL strings land in the same directory only if they hash to the same
/// 64-bit FNV-1a value — the hash runs over the *entire* URL, not the truncated
/// human-readable prefix, so a collision needs a genuine hash collision, not merely two
/// URLs that sanitize the same way. Even that vanishingly unlikely case fails safely
/// rather than silently: [`vcs::process::GitRunner::clone_bare`] refuses to clone into a
/// directory that already exists, so a collision surfaces as a loud
/// [`vcs::process::RemoteError::DestinationExists`], never two repositories' histories
/// merged into one directory.
pub fn remote_cache_dir(url: &str, root: &Path) -> PathBuf {
    let slug = sanitize_for_filesystem(url);
    let hash = fnv1a64(url.as_bytes());
    root.join(format!("{slug}-{hash:016x}"))
}

/// Why a pasted string cannot be added as a remote project.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RemoteUrlError {
    Empty,
    UnsupportedScheme,
    Malformed,
}

impl std::fmt::Display for RemoteUrlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            RemoteUrlError::Empty => "paste a repository URL first",
            RemoteUrlError::UnsupportedScheme => {
                "gitr only clones public http(s) repositories — ssh and other URLs are not supported"
            }
            RemoteUrlError::Malformed => "that doesn't look like a repository URL",
        })
    }
}

impl std::error::Error for RemoteUrlError {}

/// Validates `raw` as a public repository URL gitr can clone, returning it trimmed.
///
/// Only `http://` and `https://` are accepted. `git@host:owner/repo.git` and `ssh://`
/// URLs are rejected outright rather than attempted: `GIT_TERMINAL_PROMPT=0`
/// (`crates/vcs/src/process/runner.rs`) stops `git` itself from blocking on a credential
/// prompt, but it has no effect on ssh's own host-key confirmation prompt, and an
/// unknown host over ssh would hang the clone with nothing on screen to explain why.
/// Restricting this workstream to http(s) — a legitimate reading of "public
/// repositories" — avoids that hang entirely rather than working around it.
pub fn validate_remote_url(raw: &str) -> Result<String, RemoteUrlError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(RemoteUrlError::Empty);
    }
    if trimmed
        .chars()
        .any(|ch| ch.is_whitespace() || ch.is_control())
    {
        return Err(RemoteUrlError::Malformed);
    }

    let lower = trimmed.to_lowercase();
    let rest = if let Some(rest) = lower.strip_prefix("https://") {
        rest
    } else if let Some(rest) = lower.strip_prefix("http://") {
        rest
    } else {
        return Err(RemoteUrlError::UnsupportedScheme);
    };

    let authority = rest.split('/').next().unwrap_or("");
    if authority.is_empty() {
        return Err(RemoteUrlError::Malformed);
    }

    Ok(trimmed.to_string())
}

/// A human-readable "last synchronised" caption for a remote project, relative to `now`.
///
/// `now` is a parameter rather than read from [`SystemTime::now`] internally so this stays
/// testable without a clock: [`crate::sidebar::selector`] is the only real caller, and it
/// passes the actual current time.
pub fn format_last_synchronised(last: Option<SystemTime>, now: SystemTime) -> String {
    let Some(last) = last else {
        return "Never synchronised".to_string();
    };
    let elapsed = now.duration_since(last).unwrap_or_default();
    let secs = elapsed.as_secs();

    if secs < 60 {
        "Synced just now".to_string()
    } else if secs < 3_600 {
        format!("Synced {}m ago", secs / 60)
    } else if secs < 86_400 {
        format!("Synced {}h ago", secs / 3_600)
    } else if secs < 604_800 {
        format!("Synced {}d ago", secs / 86_400)
    } else {
        format!("Synced {}w ago", secs / 604_800)
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

    /// Records that the remote project named by `source` was synchronised at `when`.
    /// Does nothing, and returns `false`, for a source this list has never seen, or for a
    /// [`ProjectSource::Local`] source — synchronising is a remote-only concept.
    ///
    /// When `source` is the active project, `self.active` is refreshed to the
    /// newly-stamped source too. Nothing in this module depends on that refresh — the
    /// [`ProjectSource`] equality used throughout already ignores `last_synchronised` — but
    /// it keeps `self.active` from being the one place in this list still holding a stale
    /// timestamp after a fetch a caller might reasonably expect it to reflect.
    pub fn mark_synchronised(&mut self, source: &ProjectSource, when: SystemTime) -> bool {
        let Some(project) = self
            .projects
            .iter_mut()
            .find(|project| &project.source == source)
        else {
            return false;
        };

        let ProjectSource::Remote(remote) = &mut project.source else {
            return false;
        };
        remote.last_synchronised = Some(when);

        if self.active.as_ref() == Some(source) {
            self.active = Some(project.source.clone());
        }
        true
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

    fn remote_project(name: &str) -> Project {
        Project::remote(
            format!("https://example.com/{name}.git"),
            PathBuf::from(format!("/cache/{name}")),
        )
    }

    #[test]
    fn a_remote_source_is_identified_by_url_and_cache_dir_not_by_sync_time() {
        let mut a = remote_project("cargo");
        let mut b = a.clone();
        let ProjectSource::Remote(remote_a) = &mut a.source else {
            unreachable!()
        };
        remote_a.last_synchronised = Some(std::time::SystemTime::UNIX_EPOCH);
        let ProjectSource::Remote(remote_b) = &mut b.source else {
            unreachable!()
        };
        remote_b.last_synchronised = None;

        assert_eq!(a.source, b.source);
    }

    #[test]
    fn a_remote_source_with_a_different_url_is_not_equal() {
        let a = remote_project("cargo");
        let other = Project::remote(
            "https://example.com/other.git".to_string(),
            PathBuf::from("/cache/cargo"),
        );
        assert_ne!(a.source, other.source);
    }

    #[test]
    fn a_local_source_never_equals_a_remote_source() {
        let local = project("cargo");
        let remote = remote_project("cargo");
        assert_ne!(local.source, remote.source);
    }

    #[test]
    fn a_project_list_round_trips_a_remote_source_through_json() {
        let mut remote = remote_project("cargo");
        let ProjectSource::Remote(inner) = &mut remote.source else {
            unreachable!()
        };
        inner.last_synchronised = Some(std::time::SystemTime::UNIX_EPOCH);

        let list = ProjectList {
            active: Some(remote.source.clone()),
            projects: vec![remote],
        };

        let json = serde_json::to_string(&list).expect("list must serialise");
        let restored: ProjectList = serde_json::from_str(&json).expect("list must deserialise");
        assert_eq!(restored, list);
    }

    #[test]
    fn marking_a_remote_project_synchronised_stamps_only_that_project() {
        let mut list = ProjectList::default();
        list.add_or_activate(remote_project("cargo"));
        list.add_or_activate(remote_project("gitr"));

        let when = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_000);
        let updated = list.mark_synchronised(&remote_project("cargo").source, when);

        assert!(updated);
        let cargo = list
            .projects
            .iter()
            .find(|p| p.name == "cargo")
            .expect("cargo must still be listed");
        let ProjectSource::Remote(remote) = &cargo.source else {
            panic!("cargo must still be a remote source");
        };
        assert_eq!(remote.last_synchronised, Some(when));

        let gitr = list
            .projects
            .iter()
            .find(|p| p.name == "gitr")
            .expect("gitr must still be listed");
        let ProjectSource::Remote(remote) = &gitr.source else {
            panic!("gitr must still be a remote source");
        };
        assert_eq!(remote.last_synchronised, None);
    }

    #[test]
    fn marking_an_unknown_source_synchronised_changes_nothing_and_reports_false() {
        let mut list = ProjectList::default();
        list.add_or_activate(remote_project("cargo"));

        let updated = list.mark_synchronised(&remote_project("gone").source, SystemTime::now());
        assert!(!updated);
    }

    #[test]
    fn marking_a_local_source_synchronised_is_a_no_op() {
        let mut list = ProjectList::default();
        list.add_or_activate(project("cargo"));

        let updated = list.mark_synchronised(&project("cargo").source, SystemTime::now());
        assert!(!updated);
    }

    /// The wiring bug this exists to catch: if `ProjectSource`'s equality ever included
    /// `last_synchronised` again, `self.active`'s snapshot (taken before the fetch) would
    /// stop matching the just-updated project in `self.projects`, and `active_project`
    /// would silently fall back to the first project in the list instead of the one that
    /// was actually just synchronised.
    #[test]
    fn active_project_still_resolves_correctly_after_being_marked_synchronised() {
        let mut list = ProjectList::default();
        list.add_or_activate(remote_project("cargo"));
        list.add_or_activate(remote_project("gitr"));
        list.activate(&remote_project("cargo").source);

        list.mark_synchronised(&remote_project("cargo").source, SystemTime::now());

        assert_eq!(
            list.active_project().map(|p| p.name.as_str()),
            Some("cargo")
        );
    }

    #[test]
    fn filter_projects_with_an_empty_query_returns_every_project() {
        let projects = vec![project("cargo"), project("gitr"), project("swarm_rs")];
        assert_eq!(filter_projects(&projects, "").len(), 3);
        assert_eq!(filter_projects(&projects, "   ").len(), 3);
    }

    #[test]
    fn filter_projects_matches_case_insensitively_across_the_whole_list() {
        let projects: Vec<Project> = (0..(MAX_VISIBLE_PROJECTS + 3))
            .map(|ix| project(&format!("project-{ix}")))
            .chain(std::iter::once(project("special-CARGO-repo")))
            .collect();

        let matches = filter_projects(&projects, "cargo");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].name, "special-CARGO-repo");
    }

    #[test]
    fn filter_projects_reaches_a_project_far_past_the_visible_cap() {
        let projects: Vec<Project> = (0..(MAX_VISIBLE_PROJECTS * 4))
            .map(|ix| project(&format!("repo-{ix}")))
            .collect();

        let target = format!("repo-{}", MAX_VISIBLE_PROJECTS * 4 - 1);
        let matches = filter_projects(&projects, &target);

        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].name, target);
    }

    #[test]
    fn the_visible_cap_is_five() {
        assert_eq!(MAX_VISIBLE_PROJECTS, 5);
    }

    #[test]
    fn remote_display_name_strips_the_git_suffix_from_the_last_path_segment() {
        assert_eq!(
            remote_display_name("https://github.com/rust-lang/cargo.git"),
            "cargo"
        );
        assert_eq!(
            remote_display_name("https://github.com/rust-lang/cargo"),
            "cargo"
        );
    }

    #[test]
    fn remote_display_name_ignores_a_trailing_slash() {
        assert_eq!(
            remote_display_name("https://github.com/rust-lang/cargo/"),
            "cargo"
        );
    }

    #[test]
    fn remote_display_name_uses_the_host_when_the_url_has_no_path_segment() {
        assert_eq!(remote_display_name("https://example.com"), "example.com");
    }

    #[test]
    fn remote_cache_dir_is_deterministic_for_the_same_url() {
        let root = PathBuf::from("/cache");
        let url = "https://github.com/rust-lang/cargo.git";
        assert_eq!(remote_cache_dir(url, &root), remote_cache_dir(url, &root));
    }

    #[test]
    fn remote_cache_dir_nests_under_the_given_root() {
        let root = PathBuf::from("/cache/gitr/remotes");
        let dir = remote_cache_dir("https://github.com/rust-lang/cargo.git", &root);
        assert!(dir.starts_with(&root));
    }

    #[test]
    fn two_different_urls_never_produce_the_same_cache_dir() {
        let root = PathBuf::from("/cache");
        let urls = [
            "https://github.com/rust-lang/cargo.git",
            "https://github.com/rust-lang/cargo",
            "https://gitlab.com/rust-lang/cargo.git",
            "https://github.com/rust-lang/cargo2.git",
            "http://github.com/rust-lang/cargo.git",
        ];

        let mut dirs: Vec<PathBuf> = urls
            .iter()
            .map(|url| remote_cache_dir(url, &root))
            .collect();
        let original_len = dirs.len();
        dirs.sort();
        dirs.dedup();
        assert_eq!(dirs.len(), original_len);
    }

    #[test]
    fn remote_cache_dir_sanitizes_filesystem_unsafe_characters() {
        let root = PathBuf::from("/cache");
        let dir = remote_cache_dir("https://example.com/a b/c?d=e#f/", &root);
        let component = dir
            .file_name()
            .and_then(|name| name.to_str())
            .expect("cache dir must have a final path component");

        assert!(!component.contains(' '));
        assert!(!component.contains('/'));
        assert!(!component.contains('?'));
        assert!(!component.contains('#'));
        assert!(!component.contains('='));
        assert_eq!(dir.parent(), Some(root.as_path()));
    }

    #[test]
    fn remote_cache_dir_handles_a_url_with_no_safe_characters_at_all() {
        let root = PathBuf::from("/cache");
        let dir = remote_cache_dir("https://???///", &root);
        assert!(dir.starts_with(&root));
        assert!(dir.file_name().is_some());
    }

    #[test]
    fn validate_remote_url_accepts_http_and_https() {
        assert_eq!(
            validate_remote_url("https://github.com/rust-lang/cargo"),
            Ok("https://github.com/rust-lang/cargo".to_string())
        );
        assert_eq!(
            validate_remote_url("  http://example.com/repo.git  "),
            Ok("http://example.com/repo.git".to_string())
        );
    }

    #[test]
    fn validate_remote_url_rejects_an_empty_string() {
        assert_eq!(validate_remote_url(""), Err(RemoteUrlError::Empty));
        assert_eq!(validate_remote_url("   "), Err(RemoteUrlError::Empty));
    }

    #[test]
    fn validate_remote_url_rejects_ssh_urls() {
        assert_eq!(
            validate_remote_url("ssh://git@github.com/rust-lang/cargo.git"),
            Err(RemoteUrlError::UnsupportedScheme)
        );
    }

    #[test]
    fn validate_remote_url_rejects_scp_like_ssh_syntax() {
        assert_eq!(
            validate_remote_url("git@github.com:rust-lang/cargo.git"),
            Err(RemoteUrlError::UnsupportedScheme)
        );
    }

    #[test]
    fn validate_remote_url_rejects_other_unsupported_schemes() {
        assert_eq!(
            validate_remote_url("ftp://example.com/repo.git"),
            Err(RemoteUrlError::UnsupportedScheme)
        );
    }

    #[test]
    fn validate_remote_url_rejects_whitespace_inside_the_url() {
        assert_eq!(
            validate_remote_url("https://example.com/rust lang/cargo"),
            Err(RemoteUrlError::Malformed)
        );
    }

    #[test]
    fn validate_remote_url_rejects_a_bare_scheme_with_no_authority() {
        assert_eq!(
            validate_remote_url("https://"),
            Err(RemoteUrlError::Malformed)
        );
    }

    #[test]
    fn format_last_synchronised_reports_never_for_none() {
        assert_eq!(
            format_last_synchronised(None, SystemTime::now()),
            "Never synchronised"
        );
    }

    #[test]
    fn format_last_synchronised_reports_minutes_hours_days_and_weeks() {
        let epoch = SystemTime::UNIX_EPOCH;
        let now = epoch + std::time::Duration::from_secs(10 * 24 * 3_600);

        assert_eq!(
            format_last_synchronised(Some(now - std::time::Duration::from_secs(30)), now),
            "Synced just now"
        );
        assert_eq!(
            format_last_synchronised(Some(now - std::time::Duration::from_secs(5 * 60)), now),
            "Synced 5m ago"
        );
        assert_eq!(
            format_last_synchronised(Some(now - std::time::Duration::from_secs(3 * 3_600)), now),
            "Synced 3h ago"
        );
        assert_eq!(
            format_last_synchronised(Some(now - std::time::Duration::from_secs(2 * 86_400)), now),
            "Synced 2d ago"
        );
        assert_eq!(format_last_synchronised(Some(epoch), now), "Synced 1w ago");
    }
}

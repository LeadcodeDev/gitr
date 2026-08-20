use std::collections::HashSet;

use domain::{CommitSummary, HistoryRequest, HistoryScope, Parents, RepositoryError};
use gix::refs::Category;
use gix::traverse::commit::{Parents as TraverseParents, topo};

use super::convert::{to_domain_id, to_signature};
use super::error::{backend, backend_boxed, find_commit, find_reference};

pub(crate) fn walk(
    repo: &gix::Repository,
    request: &HistoryRequest,
) -> Result<Vec<CommitSummary>, RepositoryError> {
    let tips = resolve_tips(repo, &request.scope)?;
    let topo_walk = topo::Builder::new(&repo.objects)
        .with_tips(tips)
        .sorting(topo::Sorting::DateOrder)
        .parents(TraverseParents::All)
        .build()
        .map_err(|err| backend("starting history walk", err))?;

    let mut summaries = Vec::new();
    for info in topo_walk {
        let info = info.map_err(|err| backend("walking history", err))?;
        let commit = repo
            .find_commit(info.id)
            .map_err(|err| find_commit(info.id, err))?;
        let message = commit
            .message()
            .map_err(|err| backend(format!("decoding message of commit {}", info.id), err))?;
        let author = to_signature(
            commit
                .author()
                .map_err(|err| backend(format!("decoding author of commit {}", info.id), err))?,
        )?;
        let parent_ids: Vec<_> = info.parent_ids.iter().map(|id| to_domain_id(*id)).collect();

        summaries.push(CommitSummary {
            id: to_domain_id(info.id),
            parents: Parents::from_ids(&parent_ids),
            summary: message.summary().to_string(),
            author,
        });
        if request.limit.is_some_and(|limit| summaries.len() >= limit) {
            break;
        }
    }
    Ok(summaries)
}

/// Which reference categories seed a walk for each [`HistoryScope`].
///
/// `LocalReferences` deliberately excludes [`Category::RemoteBranch`]: those are mirrors of
/// a remote's history, not local work, and the whole point of the distinction from
/// `AllReferences` is to be able to tell the two apart.
fn resolve_tips(
    repo: &gix::Repository,
    scope: &HistoryScope,
) -> Result<Vec<gix::ObjectId>, RepositoryError> {
    match scope {
        HistoryScope::Single(reference) => Ok(vec![peel_named(repo, &reference.full_name())?]),
        HistoryScope::LocalReferences => collect_tips(repo, |category| {
            matches!(category, Category::LocalBranch | Category::Tag)
        }),
        HistoryScope::AllReferences => collect_tips(repo, |category| {
            matches!(
                category,
                Category::LocalBranch | Category::RemoteBranch | Category::Tag
            )
        }),
    }
}

/// Collects the tips matching `include`, each appearing exactly once.
///
/// Deduplication is load-bearing, not tidiness. Several references routinely resolve to the
/// same commit — `main` and `origin/main` after a push, a tag on a branch tip — and a
/// repeated identifier makes `gix`'s topological traversal terminate early: on a repository
/// with 46 references resolving to 32 distinct commits, the walk returned 139 of 328
/// commits, dropping ancestors of tips it had itself been given. Deduplicated, it returns
/// all 328. `gix` 0.86; a date-ordered or breadth-first walk over the same duplicated tips
/// is unaffected.
///
/// First-seen order is preserved so a walk is reproducible across runs.
fn collect_tips(
    repo: &gix::Repository,
    include: impl Fn(Category) -> bool,
) -> Result<Vec<gix::ObjectId>, RepositoryError> {
    let platform = repo
        .references()
        .map_err(|err| backend("listing references for history", err))?;
    let iter = platform
        .all()
        .map_err(|err| backend("listing references for history", err))?;

    let mut tips = Vec::new();
    let mut seen = HashSet::new();
    for reference in iter {
        let mut reference =
            reference.map_err(|err| backend_boxed("reading a reference for history", err))?;
        let Some((category, _)) = reference.name().category_and_short_name() else {
            continue;
        };
        if !include(category) {
            continue;
        }
        let id = reference
            .peel_to_id()
            .map_err(|err| backend(format!("resolving {}", reference.name()), err))?
            .detach();
        if seen.insert(id) {
            tips.push(id);
        }
    }

    if let Some(head_target) = repo
        .head()
        .map_err(|err| backend("reading HEAD for history", err))?
        .id()
    {
        let head_target = head_target.detach();
        if seen.insert(head_target) {
            tips.push(head_target);
        }
    }
    Ok(tips)
}

fn peel_named(repo: &gix::Repository, full_name: &str) -> Result<gix::ObjectId, RepositoryError> {
    let mut reference = repo
        .find_reference(full_name)
        .map_err(|err| find_reference(full_name, err))?;
    Ok(reference
        .peel_to_id()
        .map_err(|err| backend(format!("resolving {full_name}"), err))?
        .detach())
}

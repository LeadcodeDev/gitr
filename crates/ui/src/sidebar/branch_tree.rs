//! Groups references into a slash-nested tree for the sidebar.
//!
//! Pure and gpui-free: [`Reference::short_name`] such as `chantier/register-skill` or
//! `origin/chantier/register-skill` is a path, and this groups paths that share a prefix
//! under one node — exactly what the sidebar's `Branches` and `Remotes` sections render.
//! A remote's own name (`origin`) falls out of this for free: it is just the first
//! segment of a remote branch's short name.

use domain::Reference;

/// One node of a reference tree, addressed by `segment` — one `/`-delimited component of
/// a [`Reference::short_name`].
///
/// `reference` is `Some` when `segment` is itself a valid reference, not only a
/// namespace: Git allows both `release` and `release/1.0` to exist as branches at once,
/// so a node can carry a reference *and* still have children.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefTreeNode {
    pub segment: String,
    pub reference: Option<Reference>,
    pub children: Vec<RefTreeNode>,
}

impl RefTreeNode {
    fn new(segment: String) -> Self {
        Self {
            segment,
            reference: None,
            children: Vec::new(),
        }
    }
}

/// Builds the forest of [`RefTreeNode`] trees for `references`, nested on `/` and sorted
/// by segment at every level so the sidebar's rendering is stable across re-renders.
pub fn group_by_path(references: impl IntoIterator<Item = Reference>) -> Vec<RefTreeNode> {
    let mut roots: Vec<RefTreeNode> = Vec::new();
    for reference in references {
        let path = reference.short_name();
        let segments: Vec<&str> = path.split('/').collect();
        insert(&mut roots, &segments, reference);
    }
    sort(&mut roots);
    roots
}

fn insert(nodes: &mut Vec<RefTreeNode>, segments: &[&str], reference: Reference) {
    let Some((head, rest)) = segments.split_first() else {
        return;
    };

    let index = match nodes.iter().position(|node| node.segment == *head) {
        Some(index) => index,
        None => {
            nodes.push(RefTreeNode::new((*head).to_string()));
            nodes.len() - 1
        }
    };

    if rest.is_empty() {
        nodes[index].reference = Some(reference);
    } else {
        insert(&mut nodes[index].children, rest, reference);
    }
}

fn sort(nodes: &mut [RefTreeNode]) {
    nodes.sort_by(|a, b| a.segment.cmp(&b.segment));
    for node in nodes.iter_mut() {
        sort(&mut node.children);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::BranchName;

    fn local(name: &str) -> Reference {
        Reference::LocalBranch(BranchName::new(name).unwrap())
    }

    fn segments(nodes: &[RefTreeNode]) -> Vec<&str> {
        nodes.iter().map(|node| node.segment.as_str()).collect()
    }

    #[test]
    fn flat_names_stay_flat_and_sorted() {
        let tree = group_by_path([local("main"), local("develop")]);

        assert_eq!(segments(&tree), vec!["develop", "main"]);
        assert!(tree.iter().all(|node| node.children.is_empty()));
        assert_eq!(tree[1].reference, Some(local("main")));
    }

    #[test]
    fn slash_separated_names_nest_under_a_shared_prefix() {
        let tree = group_by_path([
            local("chantier/register-skill"),
            local("chantier/m1-read-and-visualise"),
        ]);

        assert_eq!(tree.len(), 1);
        let chantier = &tree[0];
        assert_eq!(chantier.segment, "chantier");
        assert!(chantier.reference.is_none());
        assert_eq!(
            segments(&chantier.children),
            vec!["m1-read-and-visualise", "register-skill"]
        );
    }

    #[test]
    fn a_name_can_be_both_a_namespace_and_a_leaf() {
        let tree = group_by_path([local("release"), local("release/1.0")]);

        assert_eq!(tree.len(), 1);
        let release = &tree[0];
        assert_eq!(release.reference, Some(local("release")));
        assert_eq!(segments(&release.children), vec!["1.0"]);
    }

    #[test]
    fn children_are_sorted_by_segment_at_every_level() {
        let tree = group_by_path([
            local("zeta"),
            local("alpha"),
            local("mid/b"),
            local("mid/a"),
        ]);

        assert_eq!(segments(&tree), vec!["alpha", "mid", "zeta"]);
        let mid = tree.iter().find(|node| node.segment == "mid").unwrap();
        assert_eq!(segments(&mid.children), vec!["a", "b"]);
    }

    #[test]
    fn a_remote_branchs_short_name_nests_under_its_remote_for_free() {
        let origin_main = Reference::RemoteBranch {
            remote: domain::RemoteName::new("origin").unwrap(),
            branch: BranchName::new("main").unwrap(),
        };
        let origin_feature = Reference::RemoteBranch {
            remote: domain::RemoteName::new("origin").unwrap(),
            branch: BranchName::new("feat/x").unwrap(),
        };

        let tree = group_by_path([origin_main, origin_feature]);

        assert_eq!(tree.len(), 1);
        assert_eq!(tree[0].segment, "origin");
        assert_eq!(segments(&tree[0].children), vec!["feat", "main"]);
    }

    #[test]
    fn an_empty_input_produces_an_empty_forest() {
        assert_eq!(group_by_path([]), Vec::new());
    }
}

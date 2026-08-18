use crate::object_id::ObjectId;

/// A commit's parents, shaped so the common cases cost no allocation.
///
/// Ninety-nine percent of a history is [`Parents::Linear`], and the graph layout walks
/// parents once per commit per frame budget. A `Vec` would put a heap allocation and a
/// pointer chase on that path for every commit; only the octopus case, which is rare
/// enough to be a curiosity, pays one here.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Parents {
    Root,
    Linear(ObjectId),
    Merge(ObjectId, ObjectId),
    Octopus(Vec<ObjectId>),
}

impl Parents {
    pub fn len(&self) -> usize {
        match self {
            Self::Root => 0,
            Self::Linear(_) => 1,
            Self::Merge(_, _) => 2,
            Self::Octopus(ids) => ids.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        matches!(self, Self::Root)
    }

    pub fn is_merge(&self) -> bool {
        self.len() > 1
    }

    pub fn iter(&self) -> impl Iterator<Item = ObjectId> + '_ {
        let (inline, spilled): (&[ObjectId], &[ObjectId]) = match self {
            Self::Root => (&[], &[]),
            Self::Linear(first) => (std::slice::from_ref(first), &[]),
            Self::Merge(first, second) => {
                (std::slice::from_ref(first), std::slice::from_ref(second))
            }
            Self::Octopus(ids) => (&[], ids.as_slice()),
        };
        inline.iter().chain(spilled.iter()).copied()
    }

    /// The parent whose lane a commit continues. `None` for a root commit.
    pub fn first(&self) -> Option<ObjectId> {
        self.iter().next()
    }

    /// Builds the narrowest representation that holds `ids`.
    pub fn from_ids(ids: &[ObjectId]) -> Self {
        match ids {
            [] => Self::Root,
            [only] => Self::Linear(*only),
            [first, second] => Self::Merge(*first, *second),
            many => Self::Octopus(many.to_vec()),
        }
    }
}

/// Seconds since the Unix epoch, with the offset the author's clock was set to.
///
/// The offset is kept because Git records it and a client that discards it shows a
/// different timestamp than `git log` does for the same commit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Timestamp {
    pub seconds: i64,
    pub offset_minutes: i16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Signature {
    pub name: String,
    pub email: String,
    pub time: Timestamp,
}

/// The columns of one history row, plus the parent links the graph layout needs.
///
/// Deliberately excludes the commit body: a history load reads every row, and bodies
/// dominate the byte count of a large repository's log.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommitSummary {
    pub id: ObjectId,
    pub parents: Parents,
    pub summary: String,
    pub author: Signature,
}

/// Everything the detail panel shows for a single commit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Commit {
    pub id: ObjectId,
    pub parents: Parents,
    pub summary: String,
    pub body: String,
    pub author: Signature,
    pub committer: Signature,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn id(nibble: char) -> ObjectId {
        ObjectId::from_str(&nibble.to_string().repeat(40)).unwrap()
    }

    #[test]
    fn from_ids_picks_the_narrowest_shape() {
        assert_eq!(Parents::from_ids(&[]), Parents::Root);
        assert_eq!(Parents::from_ids(&[id('a')]), Parents::Linear(id('a')));
        assert_eq!(
            Parents::from_ids(&[id('a'), id('b')]),
            Parents::Merge(id('a'), id('b'))
        );
        assert_eq!(
            Parents::from_ids(&[id('a'), id('b'), id('c')]),
            Parents::Octopus(vec![id('a'), id('b'), id('c')])
        );
    }

    #[test]
    fn iter_yields_every_parent_in_order() {
        let cases = [
            (Parents::Root, vec![]),
            (Parents::Linear(id('a')), vec![id('a')]),
            (Parents::Merge(id('a'), id('b')), vec![id('a'), id('b')]),
            (
                Parents::Octopus(vec![id('a'), id('b'), id('c')]),
                vec![id('a'), id('b'), id('c')],
            ),
        ];
        for (parents, expected) in cases {
            assert_eq!(parents.iter().collect::<Vec<_>>(), expected);
        }
    }

    #[test]
    fn len_agrees_with_iter() {
        let cases = [
            Parents::Root,
            Parents::Linear(id('a')),
            Parents::Merge(id('a'), id('b')),
            Parents::Octopus(vec![id('a'), id('b'), id('c')]),
        ];
        for parents in cases {
            assert_eq!(parents.len(), parents.iter().count());
        }
    }

    #[test]
    fn only_multi_parent_commits_are_merges() {
        assert!(!Parents::Root.is_merge());
        assert!(!Parents::Linear(id('a')).is_merge());
        assert!(Parents::Merge(id('a'), id('b')).is_merge());
        assert!(Parents::Octopus(vec![id('a'), id('b'), id('c')]).is_merge());
    }

    #[test]
    fn first_is_the_lane_continuing_parent() {
        assert_eq!(Parents::Root.first(), None);
        assert_eq!(Parents::Merge(id('a'), id('b')).first(), Some(id('a')));
    }
}

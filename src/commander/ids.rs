/*!
Helper structs [ChangeId] and [CommitId]
*/
use std::ffi::OsStr;
use std::fmt::Display;

/// Wrapper around change ID.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct ChangeId(pub String);

impl ChangeId {
    pub fn as_os_str(&self) -> &OsStr {
        OsStr::new(&self.0)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn as_string(&self) -> String {
        self.0.to_owned()
    }
}

impl AsRef<OsStr> for ChangeId {
    fn as_ref(&self) -> &OsStr {
        self.as_os_str()
    }
}

impl Display for ChangeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Wrapper around commit ID.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct CommitId(pub String);

impl CommitId {
    pub fn as_os_str(&self) -> &OsStr {
        OsStr::new(&self.0)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    // pub fn as_string(&self) -> String {
    //     self.0.to_owned()
    // }
}

impl AsRef<OsStr> for CommitId {
    fn as_ref(&self) -> &OsStr {
        self.as_os_str()
    }
}

impl Display for CommitId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Build a revset expression that is the union of the given commit IDs:
/// `id1 | id2 | id3`. A single id returns the bare id. Empty input is a
/// misuse — callers must fall back to a single commit (typically the
/// current head) before calling.
pub fn commit_revset_union(ids: &[CommitId]) -> String {
    ids.iter()
        .map(CommitId::as_str)
        .collect::<Vec<_>>()
        .join(" | ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commit_revset_union_single() {
        let ids = [CommitId("abc".to_owned())];
        assert_eq!(commit_revset_union(&ids), "abc");
    }

    #[test]
    fn commit_revset_union_multiple() {
        let ids = [
            CommitId("abc".to_owned()),
            CommitId("def".to_owned()),
            CommitId("ghi".to_owned()),
        ];
        assert_eq!(commit_revset_union(&ids), "abc | def | ghi");
    }
}

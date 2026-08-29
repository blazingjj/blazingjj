/*!
Revset expressions for jj revision arguments
*/
use std::fmt::Display;

use crate::commander::ids::ChangeId;
use crate::commander::ids::CommitId;

/// A jj revset expression.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Revset(String);

impl Revset {
    /// An arbitrary revset expression. Only the tests build one, every
    /// caller having something more specific to hand.
    #[cfg(test)]
    pub fn expression(expression: impl Into<String>) -> Self {
        Self(expression.into())
    }

    /// The union of the given revsets, or [None] if there are none.
    pub fn union(revsets: impl IntoIterator<Item = impl Into<Revset>>) -> Option<Self> {
        let expressions: Vec<_> = revsets.into_iter().map(|revset| revset.into().0).collect();
        if expressions.is_empty() {
            return None;
        }

        Some(Self(expressions.join(" | ")))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&ChangeId> for Revset {
    fn from(id: &ChangeId) -> Self {
        Self(id.as_string())
    }
}

impl From<&CommitId> for Revset {
    fn from(id: &CommitId) -> Self {
        Self(id.as_str().to_owned())
    }
}

impl Display for Revset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn union_of_nothing() {
        assert_eq!(Revset::union(Vec::<Revset>::new()), None);
    }

    #[test]
    fn union_of_one_is_bare() {
        let ids = [CommitId("abc".to_owned())];
        assert_eq!(Revset::union(&ids), Some(Revset::expression("abc")));
    }

    #[test]
    fn union_of_several() {
        let ids = [
            CommitId("abc".to_owned()),
            CommitId("def".to_owned()),
            CommitId("ghi".to_owned()),
        ];
        assert_eq!(
            Revset::union(&ids),
            Some(Revset::expression("abc | def | ghi"))
        );
    }

    #[test]
    fn union_of_mixed_ids() {
        let commit = CommitId("abc".to_owned());
        let change = ChangeId("xyz".to_owned());
        assert_eq!(
            Revset::union([Revset::from(&commit), Revset::from(&change)]),
            Some(Revset::expression("abc | xyz"))
        );
    }
}

use pubgrub::range::Range as R;
use pubgrub::version_set::VersionSet;
use std::collections::Bound;
use std::fmt::{Display, Formatter};
use std::ops::Deref;

use pep440_rs::Version;

/// A [`Range<Version>`] with a Python-specific `Display` implementation.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub(crate) struct Range(R<Version>);

impl VersionSet for Range {
    type V = Version;

    fn empty() -> Self {
        Self(R::empty())
    }

    fn singleton(v: Self::V) -> Self {
        Self(R::singleton(v))
    }

    fn complement(&self) -> Self {
        Self(self.0.complement())
    }

    fn intersection(&self, other: &Self) -> Self {
        Self(self.0.intersection(&other.0))
    }

    fn contains(&self, v: &Self::V) -> bool {
        self.0.contains(v)
    }

    fn full() -> Self {
        Self(R::full())
    }

    fn union(&self, other: &Self) -> Self {
        Self(self.0.union(&other.0))
    }

    fn is_disjoint(&self, other: &Self) -> bool {
        self.0.is_disjoint(&other.0)
    }

    fn subset_of(&self, other: &Self) -> bool {
        self.0.subset_of(&other.0)
    }
}

impl Range {
    /// Empty set of versions.
    pub fn empty() -> Self {
        Self(R::empty())
    }

    /// Set of all possible versions
    pub fn full() -> Self {
        Self(R::full())
    }

    /// Set of all versions higher or equal to some version
    pub fn higher_than(v: impl Into<Version>) -> Self {
        Self(R::higher_than(v))
    }

    /// Set of all versions higher to some version
    pub fn strictly_higher_than(v: impl Into<Version>) -> Self {
        Self(R::strictly_higher_than(v))
    }

    /// Set of all versions lower to some version
    pub fn strictly_lower_than(v: impl Into<Version>) -> Self {
        Self(R::strictly_lower_than(v))
    }

    /// Set of all versions lower or equal to some version
    pub fn lower_than(v: impl Into<Version>) -> Self {
        Self(R::lower_than(v))
    }

    /// Set of versions greater or equal to `v1` but less than `v2`.
    pub fn between(v1: impl Into<Version>, v2: impl Into<Version>) -> Self {
        Self(R::between(v1, v2))
    }
}

impl From<R<Version>> for Range {
    fn from(range: R<Version>) -> Self {
        Self(range)
    }
}

impl Into<R<Version>> for Range {
    fn into(self) -> R<Version> {
        self.0
    }
}

impl Deref for Range {
    type Target = R<Version>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// Python-specific [`Display`] implementation.
///
/// `|` is used as OR-operator instead of `,` since PEP 440 uses `,` as AND-operator. `==` is used
/// for single version specifiers instead of an empty prefix, again for PEP 440 where a specifier
/// needs an operator.
impl Display for Range {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if self.0.is_empty() {
            write!(f, "∅")?;
        } else {
            for (idx, segment) in self.0.iter().enumerate() {
                if idx > 0 {
                    write!(f, " | ")?;
                }
                match segment {
                    (Bound::Unbounded, Bound::Unbounded) => write!(f, "*")?,
                    (Bound::Unbounded, Bound::Included(v)) => write!(f, "<={v}")?,
                    (Bound::Unbounded, Bound::Excluded(v)) => write!(f, "<{v}")?,
                    (Bound::Included(v), Bound::Unbounded) => write!(f, ">={v}")?,
                    (Bound::Included(v), Bound::Included(b)) => {
                        if v == b {
                            write!(f, "=={v}")?
                        } else {
                            write!(f, ">={v}, <={b}")?
                        }
                    }
                    (Bound::Included(v), Bound::Excluded(b)) => write!(f, ">={v}, <{b}")?,
                    (Bound::Excluded(v), Bound::Unbounded) => write!(f, ">{v}")?,
                    (Bound::Excluded(v), Bound::Included(b)) => write!(f, ">{v}, <={b}")?,
                    (Bound::Excluded(v), Bound::Excluded(b)) => write!(f, ">{v}, <{b}")?,
                };
            }
        }
        Ok(())
    }
}

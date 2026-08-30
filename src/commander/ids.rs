/*!
Helper structs [ChangeId], [CommitId] and [OperationId]
*/
use std::ffi::OsStr;
use std::fmt::Display;

use serde::Deserialize;

/// Wrapper around change ID.
#[derive(Clone, PartialEq, Eq, Hash, Debug, Deserialize)]
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

/// How many characters of a commit id we show, which is the fewest jj
/// puts in a log.
const SHORT_COMMIT_ID_LEN: usize = 8;

/// Wrapper around commit ID.
#[derive(Clone, PartialEq, Eq, Hash, Debug, Deserialize)]
pub struct CommitId(pub String);

impl CommitId {
    pub fn as_os_str(&self) -> &OsStr {
        OsStr::new(&self.0)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The leading [SHORT_COMMIT_ID_LEN] characters of the id.
    pub fn short(&self) -> &str {
        &self.0[..SHORT_COMMIT_ID_LEN.min(self.0.len())]
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

/// How many characters of an operation id we show, which is the number
/// jj puts in an operation log.
const SHORT_OPERATION_ID_LEN: usize = 12;

/// Wrapper around operation ID.
#[derive(Clone, Default, PartialEq, Eq, Hash, Debug, Deserialize)]
pub struct OperationId(pub String);

impl OperationId {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The leading [SHORT_OPERATION_ID_LEN] characters of the id.
    pub fn short(&self) -> &str {
        &self.0[..SHORT_OPERATION_ID_LEN.min(self.0.len())]
    }
}

impl Display for OperationId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

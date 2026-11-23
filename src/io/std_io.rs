// Rust language amplification library providing multiple generic trait
// implementations, type wrappers, derive macros and other language enhancements
//
// Written in 2019-2026 by
//     Dr. Maxim Orlovsky <orlovsky@ubideco.org>
//     Martin Habovstiak <martin.habovstiak@gmail.com>
//
// To the extent possible under law, the author(s) have dedicated all
// copyright and related and neighboring rights to this software to
// the public domain worldwide. This software is distributed without
// any warranty.
//
// You should have received a copy of the MIT License
// along with this software.
// If not, see <https://opensource.org/licenses/MIT>.

use std::cmp::Ordering;
use std::io;
use std::fmt::{Debug, Display, Formatter, self};
use std::error::Error as StdError;
use std::hash::{Hash, Hasher};

/// Cloneable I/O error type preserving the [`io::ErrorKind`] and formatted
/// details.
///
/// Available only when both `std` and `derive` features are present.
///
/// # Example
/// ```
/// use amplify::io::IoError;
/// use amplify::{Error, Display, From};
///
/// #[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, From, Debug, Display, Error)]
/// enum Error {
///     #[from(std::io::Error)]
///     #[display(inner)]
///     Io(IoError),
/// }
/// ```
pub struct IoError {
    kind: io::ErrorKind,
    display: String,
    debug: String,
    details: Option<Box<dyn StdError + Send + Sync>>,
}

impl IoError {
    /// Returns [`io::ErrorKind`] of this error.
    pub fn kind(&self) -> io::ErrorKind {
        self.kind
    }
}

impl Clone for IoError {
    fn clone(&self) -> Self {
        Self {
            kind: self.kind,
            display: self.display.clone(),
            debug: self.debug.clone(),
            details: None,
        }
    }
}

impl PartialEq for IoError {
    fn eq(&self, other: &Self) -> bool {
        self.kind == other.kind && self.debug == other.debug
    }
}

impl Eq for IoError {}

impl PartialOrd for IoError {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for IoError {
    fn cmp(&self, other: &Self) -> Ordering {
        match self.kind.cmp(&other.kind) {
            Ordering::Equal => self.debug.cmp(&other.debug),
            ordering => ordering,
        }
    }
}

impl Hash for IoError {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write(self.debug.as_bytes())
    }
}

impl Display for IoError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(&self.display)
    }
}

impl Debug for IoError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(&self.debug)
    }
}

impl std::error::Error for IoError {}

impl From<io::Error> for IoError {
    fn from(err: io::Error) -> Self {
        IoError {
            kind: err.kind(),
            display: err.to_string(),
            debug: format!("{:?}", err),
            details: err.into_inner(),
        }
    }
}

impl From<io::ErrorKind> for IoError {
    fn from(kind: io::ErrorKind) -> Self {
        IoError {
            kind,
            display: kind.to_string(),
            debug: format!("{:?}", kind),
            details: None,
        }
    }
}

impl From<IoError> for io::Error {
    fn from(err: IoError) -> Self {
        match err.details {
            Some(details) => io::Error::new(err.kind, details),
            None => io::Error::from(err.kind),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn io_error_traits_formatting_and_conversions() {
        let from_kind = IoError::from(io::ErrorKind::NotFound);
        assert_eq!(from_kind.kind(), io::ErrorKind::NotFound);
        assert!(from_kind.source().is_none());

        let detailed = IoError::from(io::Error::other("alpha"));
        let detailed_same = IoError::from(io::Error::other("alpha"));
        let detailed_diff = IoError::from(io::Error::other("beta"));
        let cloned = detailed.clone();

        assert_eq!(detailed, detailed_same);
        assert_eq!(detailed, cloned);
        assert_ne!(detailed, detailed_diff);
        assert_ne!(detailed, from_kind);

        assert_eq!(detailed.partial_cmp(&detailed_same), Some(Ordering::Equal));
        assert_eq!(detailed.cmp(&detailed_diff), Ordering::Less);
        assert_ne!(from_kind.cmp(&detailed), Ordering::Equal);

        let mut set = HashSet::new();
        assert!(set.insert(detailed_same));
        assert!(!set.insert(cloned.clone()));
        assert!(set.insert(from_kind.clone()));

        assert!(format!("{detailed}").contains("alpha"));
        assert!(format!("{detailed:?}").contains("alpha"));

        let recovered_detailed: io::Error = detailed.into();
        assert_eq!(recovered_detailed.kind(), io::ErrorKind::Other);
        assert_eq!(recovered_detailed.to_string(), "alpha");

        let recovered_none: io::Error = cloned.into();
        assert_eq!(recovered_none.kind(), io::ErrorKind::Other);
        assert!(recovered_none.get_ref().is_none());
    }
}

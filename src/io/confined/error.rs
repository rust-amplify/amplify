// Rust language amplification library providing multiple generic trait
// implementations, type wrappers, derive macros and other language enhancements
//
// Written in 2019-2026 by
//     Dr. Maxim Orlovsky <orlovsky@ubideco.org>
//
// To the extent possible under law, the author(s) have dedicated all
// copyright and related and neighboring rights to this software to
// the public domain worldwide. This software is distributed without
// any warranty.
//
// You should have received a copy of the MIT License
// along with this software.
// If not, see <https://opensource.org/licenses/MIT>.

use core::fmt;
use std::error::Error as StdError;
use std::io;

use crate::io::IoError;

/// Confinement bound violation encountered during confined stream I/O or
/// finalization.
///
/// Mirrors [`confined::Error::Undersize`] and [`confined::Error::Oversize`] for
/// streaming byte transfers, and converts losslessly into [`confined::Error`],
/// [`io::Error`] (with [`io::ErrorKind::UnexpectedEof`] and this error attached
/// as the inner payload), and [`IoError`].
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum ConfinedIoError {
    /// Stream reached EOF or was finalized before transferring the required
    /// minimum number of bytes.
    Undersize {
        /// Actual number of bytes transferred so far.
        count: usize,
        /// Minimum number of bytes required by the stream confinement.
        min: usize,
    },

    /// Operation attempted to transfer more bytes than permitted by the
    /// stream's maximum confinement bound.
    Oversize {
        /// Attempted total number of bytes.
        count: usize,
        /// Maximum number of bytes permitted by the stream confinement.
        max: usize,
    },
}

impl ConfinedIoError {
    /// Extracts a [`ConfinedIoError`] payload from an [`io::Error`] if the I/O
    /// error originated from a confined stream bound violation.
    #[inline]
    pub fn from_io_error(err: &io::Error) -> Option<Self> {
        err.get_ref()?.downcast_ref::<Self>().copied()
    }
}

impl fmt::Display for ConfinedIoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Undersize { count, min } => write!(
                f,
                "stream transfer of {count} bytes is less than lower confinement boundary of {min}"
            ),
            Self::Oversize { count, max } => write!(
                f,
                "stream transfer of {count} bytes exceeds upper confinement boundary of {max}"
            ),
        }
    }
}

impl StdError for ConfinedIoError {}

impl From<ConfinedIoError> for confined::Error {
    #[inline]
    fn from(err: ConfinedIoError) -> Self {
        match err {
            ConfinedIoError::Undersize { count, min } => Self::Undersize {
                len: count,
                min_len: min,
            },
            ConfinedIoError::Oversize { count, max } => Self::Oversize {
                len: count,
                max_len: max,
            },
        }
    }
}

impl From<ConfinedIoError> for io::Error {
    #[inline]
    fn from(err: ConfinedIoError) -> Self {
        io::Error::new(io::ErrorKind::UnexpectedEof, err)
    }
}

impl From<ConfinedIoError> for IoError {
    #[inline]
    fn from(err: ConfinedIoError) -> Self {
        IoError::from(io::Error::from(err))
    }
}

/// Error returned when finalizing or releasing a confined stream whose bounds
/// are not satisfied, preserving ownership of the confined stream.
#[derive(Copy, Clone, PartialEq, Eq, Hash)]
pub struct UnfinishedIoError<S> {
    /// The confined stream that failed finalization.
    pub stream: S,
    /// The violated confinement bound.
    pub error: ConfinedIoError,
}

impl<S> UnfinishedIoError<S> {
    /// Consumes the error and returns the preserved confined stream.
    #[inline]
    pub fn into_stream(self) -> S {
        self.stream
    }

    /// Consumes the error and returns `(stream, error)`.
    #[inline]
    pub fn into_parts(self) -> (S, ConfinedIoError) {
        (self.stream, self.error)
    }
}

impl<S> fmt::Debug for UnfinishedIoError<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UnfinishedIoError")
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}

impl<S> fmt::Display for UnfinishedIoError<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.error, f)
    }
}

impl<S> StdError for UnfinishedIoError<S> {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        Some(&self.error)
    }
}

impl<S> From<UnfinishedIoError<S>> for ConfinedIoError {
    #[inline]
    fn from(err: UnfinishedIoError<S>) -> Self {
        err.error
    }
}

impl<S> From<UnfinishedIoError<S>> for confined::Error {
    #[inline]
    fn from(err: UnfinishedIoError<S>) -> Self {
        err.error.into()
    }
}

impl<S> From<UnfinishedIoError<S>> for io::Error {
    #[inline]
    fn from(err: UnfinishedIoError<S>) -> Self {
        err.error.into()
    }
}

impl<S> From<UnfinishedIoError<S>> for IoError {
    #[inline]
    fn from(err: UnfinishedIoError<S>) -> Self {
        err.error.into()
    }
}

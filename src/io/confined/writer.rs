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

use std::io;
#[cfg(any(unix, target_os = "wasi"))]
use std::os::fd::{AsFd, AsRawFd, BorrowedFd, RawFd};
#[cfg(windows)]
use std::os::windows::io::{AsRawSocket, AsSocket, BorrowedSocket, RawSocket};

use amplify_num::u24;
use confined::{Confined, U8MAX, U16MAX, U24MAX, U32MAX, ZERO};

use super::error::{ConfinedIoError, UnfinishedIoError};
use crate::io::NullIo;

/// Stream writer enforcing compile-time byte count confinement bounds
/// `MIN..=MAX`.
///
/// `ConfinedWriter` guarantees at compile time that `MIN <= MAX`, rejects any
/// write whose buffer exceeds the remaining `MAX` allowance **before** writing
/// a single byte to the underlying writer, advances its counter only by the
/// bytes actually accepted by the inner writer, and validates the lower `MIN`
/// bound on finalization via [`Self::check`], [`Self::try_release`],
/// [`Self::release_checked`], or [`Self::into_confined`].
///
/// Mutable access to the wrapped writer and [`io::Seek`] are intentionally
/// omitted so safe code cannot bypass the byte-transfer confinement invariants.
///
/// # Examples
///
/// ```
/// use std::io::Write;
/// use amplify::io::{ConfinedWriter, NonEmptyWriter};
///
/// let mut writer = NonEmptyWriter::<Vec<u8>, 4>::vec();
/// writer.write_all(&[0xCA, 0xFE]).unwrap();
/// assert_eq!(writer.count_u8(), 2);
/// let blob = writer.into_confined().unwrap();
/// assert_eq!(blob.as_slice(), &[0xCA, 0xFE]);
/// ```
///
/// Inverted bounds (`MIN > MAX`) are rejected at build time:
///
/// ```compile_fail,E0080
/// use amplify::io::ConfinedWriter;
///
/// let _ = ConfinedWriter::<Vec<u8>, 4, 3>::vec();
/// ```
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct ConfinedWriter<W, const MIN: usize, const MAX: usize> {
    pub(super) count: usize,
    pub(super) writer: W,
}

impl<W, const MIN: usize, const MAX: usize> ConfinedWriter<W, MIN, MAX> {
    const ASSERT_BOUNDS: () = {
        assert!(
            MIN <= MAX,
            "the lower bound of the stream confinement exceeds its upper bound"
        );
    };

    /// Wraps `writer` in a confined stream bounded to `MIN..=MAX` bytes.
    ///
    /// Fails to compile if `MIN > MAX`.
    #[inline]
    pub const fn new(writer: W) -> Self {
        let () = Self::ASSERT_BOUNDS;
        Self { count: 0, writer }
    }

    #[inline]
    pub(super) const fn from_raw_parts(writer: W, count: usize) -> Self {
        let () = Self::ASSERT_BOUNDS;
        assert!(
            count <= MAX,
            "confined writer count exceeds upper confinement bound"
        );
        Self { count, writer }
    }

    /// Returns the number of bytes written through this confined stream so far.
    #[inline]
    pub const fn count(&self) -> usize {
        self.count
    }

    /// Returns the number of bytes written as [`u8`].
    ///
    /// Fails to compile unless `MAX <= U8MAX`.
    #[inline]
    pub const fn count_u8(&self) -> u8 {
        const {
            assert!(
                MAX <= U8MAX,
                "`count_u8` is allowed only on streams confined to at most `U8MAX` bytes"
            );
        };
        self.count as u8
    }

    /// Returns the number of bytes written as [`u16`].
    ///
    /// Fails to compile unless `MAX <= U16MAX`.
    #[inline]
    pub const fn count_u16(&self) -> u16 {
        const {
            assert!(
                MAX <= U16MAX,
                "`count_u16` is allowed only on streams confined to at most `U16MAX` bytes"
            );
        };
        self.count as u16
    }

    /// Returns the number of bytes written as [`u24`].
    ///
    /// Fails to compile unless `MAX <= U24MAX`.
    #[inline]
    pub const fn count_u24(&self) -> u24 {
        const {
            assert!(
                MAX <= U24MAX,
                "`count_u24` is allowed only on streams confined to at most `U24MAX` bytes"
            );
        };
        u24::with(self.count as u32)
    }

    /// Returns the number of bytes written as [`u32`].
    ///
    /// Fails to compile unless `MAX <= U32MAX`.
    #[inline]
    pub const fn count_u32(&self) -> u32 {
        const {
            assert!(
                MAX <= U32MAX,
                "`count_u32` is allowed only on streams confined to at most `U32MAX` bytes"
            );
        };
        self.count as u32
    }

    /// Returns the lower confinement bound `MIN`.
    #[inline]
    pub const fn min_bound(&self) -> usize {
        MIN
    }

    /// Returns the upper confinement bound `MAX`.
    #[inline]
    pub const fn max_bound(&self) -> usize {
        MAX
    }

    /// Returns the maximum number of additional bytes that may still be
    /// written.
    #[inline]
    pub const fn remaining(&self) -> usize {
        MAX - self.count
    }

    /// Returns the minimum number of additional bytes still required to satisfy
    /// `MIN`.
    #[inline]
    pub const fn remaining_min(&self) -> usize {
        MIN.saturating_sub(self.count)
    }

    /// Returns `true` if at least `MIN` bytes have been written.
    #[inline]
    pub const fn is_satisfied(&self) -> bool {
        self.count >= MIN
    }

    /// Returns `true` if the upper limit `MAX` has been reached.
    #[inline]
    pub const fn is_exhausted(&self) -> bool {
        self.count == MAX
    }

    /// Validates that the transferred byte count currently satisfies
    /// `MIN..=MAX`.
    #[inline]
    pub const fn check(&self) -> Result<(), ConfinedIoError> {
        if self.count < MIN {
            return Err(ConfinedIoError::Undersize {
                count: self.count,
                min: MIN,
            });
        }
        Ok(())
    }

    /// Widens the confinement to looser bounds `MIN2..=MAX2` without runtime
    /// checks.
    ///
    /// Fails to compile unless `MIN2 <= MIN` and `MAX <= MAX2`.
    #[inline]
    pub fn widen<const MIN2: usize, const MAX2: usize>(self) -> ConfinedWriter<W, MIN2, MAX2> {
        const {
            assert!(
                MIN2 <= MIN && MAX <= MAX2,
                "the new bounds of the confinement do not include the current bounds"
            );
        };
        ConfinedWriter {
            count: self.count,
            writer: self.writer,
        }
    }

    /// Returns a shared reference to the underlying unconfined writer.
    #[inline]
    pub const fn as_unconfined(&self) -> &W {
        &self.writer
    }

    /// Clones and returns an unconfined copy of the underlying writer.
    #[inline]
    pub fn to_unconfined(&self) -> W
    where
        W: Clone,
    {
        self.writer.clone()
    }

    /// Releases the underlying writer after verifying that at least `MIN` bytes
    /// were written, preserving `self` inside [`UnfinishedIoError`] otherwise.
    #[inline]
    pub fn try_release(self) -> Result<W, UnfinishedIoError<Self>> {
        if let Err(error) = self.check() {
            return Err(UnfinishedIoError {
                stream: self,
                error,
            });
        }
        Ok(self.writer)
    }

    /// Releases the underlying writer, panicking if fewer than `MIN` bytes were
    /// written.
    ///
    /// # Panics
    ///
    /// Panics if `self.count() < MIN`.
    #[inline]
    pub fn release_checked(self) -> W {
        self.try_release()
            .expect("stream confinement lower bound is not satisfied")
    }

    /// Unconditionally unwraps the underlying writer without checking whether
    /// `MIN` has been satisfied (useful when aborting a stream early).
    #[inline]
    pub fn into_unconfined(self) -> W {
        self.writer
    }
}

impl<W, const MAX: usize> ConfinedWriter<W, ZERO, MAX> {
    /// Infallibly releases the underlying writer when `MIN == 0`.
    ///
    /// Streams with a non-zero lower bound (`MIN > 0`) do not provide this
    /// method; use [`Self::try_release`], [`Self::release_checked`], or
    /// [`Self::into_unconfined`] instead:
    ///
    /// ```compile_fail,E0599
    /// use amplify::io::NonEmptyWriter;
    ///
    /// let writer = NonEmptyWriter::<Vec<u8>, 8>::vec();
    /// let _ = writer.release();
    /// ```
    #[inline]
    pub fn release(self) -> W {
        self.writer
    }
}

impl<const MIN: usize, const MAX: usize> ConfinedWriter<NullIo, MIN, MAX> {
    /// Creates a confined byte-counting sink backed by [`NullIo`].
    #[inline]
    pub const fn sink() -> Self {
        Self::new(NullIo)
    }
}

impl<const MIN: usize, const MAX: usize> ConfinedWriter<Vec<u8>, MIN, MAX> {
    /// Creates a confined in-memory byte buffer writer starting from an empty
    /// [`Vec<u8>`], even when `MIN > 0`.
    ///
    /// Unlike `Confined<Vec<u8>, MIN, MAX>::new`, which is only available when
    /// `MIN == 0`, `ConfinedWriter::vec` allows streaming into a buffer and
    /// validating `MIN..=MAX` at completion via [`Self::into_confined`].
    #[inline]
    pub const fn vec() -> Self {
        Self::new(Vec::new())
    }

    /// Creates a confined in-memory byte buffer writer with initial capacity
    /// capped at `MAX`.
    #[inline]
    pub fn with_capacity(capacity: usize) -> Self {
        Self::new(Vec::with_capacity(capacity.min(MAX)))
    }

    /// Fallibly creates a confined in-memory byte buffer writer with initial
    /// capacity capped at `MAX`.
    pub fn try_with_capacity(capacity: usize) -> Result<Self, confined::Error> {
        let () = Self::ASSERT_BOUNDS;
        let mut buf = Vec::new();
        buf.try_reserve(capacity.min(MAX))
            .map_err(|_| confined::Error::CapacityExceeded { len: 0 })?;
        Ok(Self {
            count: 0,
            writer: buf,
        })
    }

    /// Finalizes the confined buffer writer into a [`Confined<Vec<u8>, MIN,
    /// MAX>`].
    ///
    /// Verifies both that this writer transferred at least `MIN` bytes and that
    /// the resulting vector's total length lies within `MIN..=MAX` (protecting
    /// against pre-populated vectors passed to [`ConfinedWriter::new`]). On
    /// failure, returns [`UnfinishedIoError`] preserving `self`.
    pub fn into_confined(self) -> Result<Confined<Vec<u8>, MIN, MAX>, UnfinishedIoError<Self>> {
        if let Err(error) = self.check() {
            return Err(UnfinishedIoError {
                stream: self,
                error,
            });
        }
        let len = self.writer.len();
        if len < MIN {
            return Err(UnfinishedIoError {
                stream: self,
                error: ConfinedIoError::Undersize {
                    count: len,
                    min: MIN,
                },
            });
        }
        if len > MAX {
            return Err(UnfinishedIoError {
                stream: self,
                error: ConfinedIoError::Oversize {
                    count: len,
                    max: MAX,
                },
            });
        }
        Ok(Confined::try_from(self.writer).expect("verified Vec<u8> length fits MIN..=MAX"))
    }
}

impl<W, const MIN: usize, const MAX: usize> From<W> for ConfinedWriter<W, MIN, MAX> {
    #[inline]
    fn from(writer: W) -> Self {
        Self::new(writer)
    }
}

impl<W: Default, const MIN: usize, const MAX: usize> Default for ConfinedWriter<W, MIN, MAX> {
    #[inline]
    fn default() -> Self {
        Self::new(W::default())
    }
}

impl<W, const MIN: usize, const MAX: usize> AsRef<W> for ConfinedWriter<W, MIN, MAX> {
    #[inline]
    fn as_ref(&self) -> &W {
        &self.writer
    }
}

impl<W: io::Write, const MIN: usize, const MAX: usize> io::Write for ConfinedWriter<W, MIN, MAX> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        if buf.len() > MAX - self.count {
            return Err(ConfinedIoError::Oversize {
                count: self.count.saturating_add(buf.len()),
                max: MAX,
            }
            .into());
        }
        let written = self.writer.write(buf)?;
        if written > buf.len() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "inner writer reported more bytes than provided buffer",
            ));
        }
        self.count += written;
        Ok(written)
    }

    fn write_vectored(&mut self, bufs: &[io::IoSlice<'_>]) -> io::Result<usize> {
        let Some(total) = bufs
            .iter()
            .try_fold(0usize, |acc, s| acc.checked_add(s.len()))
        else {
            return Err(ConfinedIoError::Oversize {
                count: usize::MAX,
                max: MAX,
            }
            .into());
        };
        if total == 0 {
            return Ok(0);
        }
        if total > MAX - self.count {
            return Err(ConfinedIoError::Oversize {
                count: self.count.saturating_add(total),
                max: MAX,
            }
            .into());
        }
        let written = self.writer.write_vectored(bufs)?;
        if written > total {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "inner writer reported more bytes than vectored buffer length",
            ));
        }
        self.count += written;
        Ok(written)
    }

    #[inline]
    fn flush(&mut self) -> io::Result<()> {
        self.writer.flush()
    }
}

#[cfg(any(unix, target_os = "wasi"))]
impl<W: AsFd, const MIN: usize, const MAX: usize> AsFd for ConfinedWriter<W, MIN, MAX> {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.writer.as_fd()
    }
}

#[cfg(any(unix, target_os = "wasi"))]
impl<W: AsRawFd, const MIN: usize, const MAX: usize> AsRawFd for ConfinedWriter<W, MIN, MAX> {
    fn as_raw_fd(&self) -> RawFd {
        self.writer.as_raw_fd()
    }
}

#[cfg(windows)]
impl<W: AsSocket, const MIN: usize, const MAX: usize> AsSocket for ConfinedWriter<W, MIN, MAX> {
    fn as_socket(&self) -> BorrowedSocket<'_> {
        self.writer.as_socket()
    }
}

#[cfg(windows)]
impl<W: AsRawSocket, const MIN: usize, const MAX: usize> AsRawSocket
    for ConfinedWriter<W, MIN, MAX>
{
    fn as_raw_socket(&self) -> RawSocket {
        self.writer.as_raw_socket()
    }
}

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

/// Stream reader enforcing compile-time byte count confinement bounds
/// `MIN..=MAX`.
///
/// `ConfinedReader` guarantees at compile time that `MIN <= MAX`, caps every
/// read to the remaining `MAX` allowance without ever reading extra bytes past
/// `MAX` from the inner reader, and reports [`ConfinedIoError::Oversize`] (via
/// [`io::ErrorKind::UnexpectedEof`]) when an operation attempts to read past
/// `MAX` or [`ConfinedIoError::Undersize`] if the inner stream reaches EOF
/// before `MIN` bytes have been transferred.
///
/// Mutable access to the wrapped reader and [`io::Seek`] are intentionally
/// omitted so safe code cannot bypass the byte-transfer confinement invariants.
///
/// # Examples
///
/// ```
/// use std::io::{Cursor, Read};
/// use amplify::io::{ConfinedIoError, ConfinedReader, TinyReader};
///
/// let blob = ConfinedReader::<_, 2, 5>::read_confined(Cursor::new(b"abcd")).unwrap();
/// assert_eq!(blob.as_slice(), b"abcd");
///
/// let mut reader = TinyReader::new(Cursor::new(b"hello"));
/// let mut buf = [0u8; 5];
/// reader.read_exact(&mut buf).unwrap();
/// assert_eq!(reader.count_u8(), 5);
/// let _cursor = reader.release();
///
/// let err = ConfinedReader::<_, 4, 8>::read_confined(Cursor::new(b"ab")).unwrap_err();
/// assert_eq!(
///     ConfinedIoError::from_io_error(&err),
///     Some(ConfinedIoError::Undersize { count: 2, min: 4 })
/// );
/// ```
///
/// Inverted bounds (`MIN > MAX`) are rejected at build time:
///
/// ```compile_fail,E0080
/// use std::io::Cursor;
/// use amplify::io::ConfinedReader;
///
/// let _ = ConfinedReader::<_, 5, 4>::new(Cursor::new(b"abcd"));
/// ```
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct ConfinedReader<R, const MIN: usize, const MAX: usize> {
    pub(super) count: usize,
    pub(super) reader: R,
}

impl<R, const MIN: usize, const MAX: usize> ConfinedReader<R, MIN, MAX> {
    const ASSERT_BOUNDS: () = {
        assert!(
            MIN <= MAX,
            "the lower bound of the stream confinement exceeds its upper bound"
        );
    };

    /// Wraps `reader` in a confined stream bounded to `MIN..=MAX` bytes.
    ///
    /// Fails to compile if `MIN > MAX`.
    #[inline]
    pub const fn new(reader: R) -> Self {
        let () = Self::ASSERT_BOUNDS;
        Self { count: 0, reader }
    }

    #[inline]
    pub(super) const fn from_raw_parts(reader: R, count: usize) -> Self {
        let () = Self::ASSERT_BOUNDS;
        assert!(
            count <= MAX,
            "confined reader count exceeds upper confinement bound"
        );
        Self { count, reader }
    }

    /// Returns the number of bytes read through this confined stream so far.
    #[inline]
    pub const fn count(&self) -> usize {
        self.count
    }

    /// Returns the number of bytes read as [`u8`].
    ///
    /// Fails to compile unless `MAX <= U8MAX`.
    ///
    /// ```compile_fail,E0080
    /// use std::io::Cursor;
    /// use amplify::io::SmallReader;
    ///
    /// let reader = SmallReader::new(Cursor::new(b"abc"));
    /// let _ = reader.count_u8();
    /// ```
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

    /// Returns the number of bytes read as [`u16`].
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

    /// Returns the number of bytes read as [`u24`].
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

    /// Returns the number of bytes read as [`u32`].
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

    /// Returns the maximum number of additional bytes that may still be read.
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

    /// Returns `true` if at least `MIN` bytes have been read.
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
    ///
    /// ```compile_fail,E0080
    /// use std::io::Cursor;
    /// use amplify::io::ConfinedReader;
    ///
    /// let reader = ConfinedReader::<_, 1, 8>::new(Cursor::new(b"abcd"));
    /// let _: ConfinedReader<_, 2, 8> = reader.widen();
    /// ```
    #[inline]
    pub fn widen<const MIN2: usize, const MAX2: usize>(self) -> ConfinedReader<R, MIN2, MAX2> {
        const {
            assert!(
                MIN2 <= MIN && MAX <= MAX2,
                "the new bounds of the confinement do not include the current bounds"
            );
        };
        ConfinedReader {
            count: self.count,
            reader: self.reader,
        }
    }

    /// Returns a shared reference to the underlying unconfined reader.
    #[inline]
    pub const fn as_unconfined(&self) -> &R {
        &self.reader
    }

    /// Clones and returns an unconfined copy of the underlying reader.
    #[inline]
    pub fn to_unconfined(&self) -> R
    where
        R: Clone,
    {
        self.reader.clone()
    }

    /// Releases the underlying reader after verifying that at least `MIN` bytes
    /// were read, preserving `self` inside [`UnfinishedIoError`] otherwise.
    #[inline]
    pub fn try_release(self) -> Result<R, UnfinishedIoError<Self>> {
        if let Err(error) = self.check() {
            return Err(UnfinishedIoError {
                stream: self,
                error,
            });
        }
        Ok(self.reader)
    }

    /// Releases the underlying reader, panicking if fewer than `MIN` bytes were
    /// read.
    ///
    /// # Panics
    ///
    /// Panics if `self.count() < MIN`.
    #[inline]
    pub fn release_checked(self) -> R {
        self.try_release()
            .expect("stream confinement lower bound is not satisfied")
    }

    /// Unconditionally unwraps the underlying reader without checking whether
    /// `MIN` has been satisfied (useful when aborting a stream early).
    #[inline]
    pub fn into_unconfined(self) -> R {
        self.reader
    }
}

impl<R, const MAX: usize> ConfinedReader<R, ZERO, MAX> {
    /// Infallibly releases the underlying reader when `MIN == 0`.
    ///
    /// Streams with a non-zero lower bound (`MIN > 0`) do not provide this
    /// method; use [`Self::try_release`], [`Self::release_checked`], or
    /// [`Self::into_unconfined`] instead:
    ///
    /// ```compile_fail,E0599
    /// use std::io::Cursor;
    /// use amplify::io::NonEmptyReader;
    ///
    /// let reader = NonEmptyReader::<_, 8>::new(Cursor::new(b"abcd"));
    /// let _ = reader.release();
    /// ```
    #[inline]
    pub fn release(self) -> R {
        self.reader
    }
}

impl<R: io::Read, const MIN: usize, const MAX: usize> ConfinedReader<R, MIN, MAX> {
    /// Reads `reader` to EOF while enforcing `MIN..=MAX` in streaming fashion,
    /// returning a [`Confined`] byte vector without ever allocating beyond
    /// `MAX`.
    pub fn read_confined(reader: R) -> io::Result<Confined<Vec<u8>, MIN, MAX>> {
        let mut confined_reader = Self::new(reader);
        let mut bytes = Vec::new();
        io::Read::read_to_end(&mut confined_reader, &mut bytes)?;
        Ok(Confined::try_from(bytes)
            .expect("ConfinedReader::read_to_end verified MIN..=MAX bounds"))
    }

    /// Reads UTF-8 text from `reader` to EOF while enforcing `MIN..=MAX` byte
    /// bounds in streaming fashion, returning a [`Confined`] [`String`].
    pub fn read_confined_string(reader: R) -> io::Result<Confined<String, MIN, MAX>> {
        let mut confined_reader = Self::new(reader);
        let mut text = String::new();
        io::Read::read_to_string(&mut confined_reader, &mut text)?;
        Ok(Confined::try_from(text)
            .expect("ConfinedReader::read_to_string verified MIN..=MAX byte bounds"))
    }
}

impl<R, const MIN: usize, const MAX: usize> From<R> for ConfinedReader<R, MIN, MAX> {
    #[inline]
    fn from(reader: R) -> Self {
        Self::new(reader)
    }
}

impl<R: Default, const MIN: usize, const MAX: usize> Default for ConfinedReader<R, MIN, MAX> {
    #[inline]
    fn default() -> Self {
        Self::new(R::default())
    }
}

impl<R, const MIN: usize, const MAX: usize> AsRef<R> for ConfinedReader<R, MIN, MAX> {
    #[inline]
    fn as_ref(&self) -> &R {
        &self.reader
    }
}

pub(super) fn fill_buf_confined<R: io::BufRead, const MIN: usize, const MAX: usize>(
    reader: &mut R,
    count: usize,
) -> io::Result<&[u8]> {
    let remaining = MAX - count;
    if remaining == 0 {
        return Err(ConfinedIoError::Oversize {
            count: MAX.saturating_add(1),
            max: MAX,
        }
        .into());
    }
    let buf = reader.fill_buf()?;
    if buf.is_empty() && count < MIN {
        return Err(ConfinedIoError::Undersize { count, min: MIN }.into());
    }
    Ok(&buf[..buf.len().min(remaining)])
}

fn read_retry_interrupted<R: io::Read>(reader: &mut R, buf: &mut [u8]) -> io::Result<usize> {
    loop {
        match reader.read(buf) {
            Ok(n) => return Ok(n),
            Err(ref e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        }
    }
}

impl<R: io::Read, const MIN: usize, const MAX: usize> io::Read for ConfinedReader<R, MIN, MAX> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        let remaining = MAX - self.count;
        if remaining == 0 {
            return Err(ConfinedIoError::Oversize {
                count: MAX.saturating_add(1),
                max: MAX,
            }
            .into());
        }
        let allowed = buf.len().min(remaining);
        let read = self.reader.read(&mut buf[..allowed])?;
        if read > allowed {
            return Err(ConfinedIoError::Oversize {
                count: self.count.saturating_add(read),
                max: MAX,
            }
            .into());
        }
        if read == 0 && self.count < MIN {
            return Err(ConfinedIoError::Undersize {
                count: self.count,
                min: MIN,
            }
            .into());
        }
        self.count += read;
        Ok(read)
    }

    fn read_vectored(&mut self, bufs: &mut [io::IoSliceMut<'_>]) -> io::Result<usize> {
        let Some(first_idx) = bufs.iter().position(|s| !s.is_empty()) else {
            return Ok(0);
        };
        let remaining = MAX - self.count;
        if remaining == 0 {
            return Err(ConfinedIoError::Oversize {
                count: MAX.saturating_add(1),
                max: MAX,
            }
            .into());
        }
        let total = bufs[first_idx..]
            .iter()
            .fold(0usize, |acc, s| acc.saturating_add(s.len()));
        if total <= remaining {
            let read = self.reader.read_vectored(bufs)?;
            if read > total {
                return Err(ConfinedIoError::Oversize {
                    count: self.count.saturating_add(read),
                    max: MAX,
                }
                .into());
            }
            if read == 0 && self.count < MIN {
                return Err(ConfinedIoError::Undersize {
                    count: self.count,
                    min: MIN,
                }
                .into());
            }
            self.count += read;
            return Ok(read);
        }
        self.read(&mut bufs[first_idx])
    }

    fn read_exact(&mut self, mut buf: &mut [u8]) -> io::Result<()> {
        if buf.len() > MAX - self.count {
            return Err(ConfinedIoError::Oversize {
                count: self.count.saturating_add(buf.len()),
                max: MAX,
            }
            .into());
        }
        while !buf.is_empty() {
            match self.read(buf) {
                Ok(0) => {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "failed to fill whole buffer",
                    ));
                }
                Ok(n) => buf = &mut buf[n..],
                Err(ref e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }

    fn read_to_end(&mut self, buf: &mut Vec<u8>) -> io::Result<usize> {
        let mut appended = 0usize;
        let mut chunk = [0u8; 4096];
        while self.count < MAX {
            let allowed = (MAX - self.count).min(chunk.len());
            let read = read_retry_interrupted(&mut self.reader, &mut chunk[..allowed])?;
            if read > allowed {
                return Err(ConfinedIoError::Oversize {
                    count: self.count.saturating_add(read),
                    max: MAX,
                }
                .into());
            }
            if read == 0 {
                if self.count < MIN {
                    return Err(ConfinedIoError::Undersize {
                        count: self.count,
                        min: MIN,
                    }
                    .into());
                }
                return Ok(appended);
            }
            buf.extend_from_slice(&chunk[..read]);
            self.count += read;
            appended += read;
        }

        // At `self.count == MAX`: verify whether the underlying stream has
        // ended.
        let mut probe = [0u8; 1];
        let extra = read_retry_interrupted(&mut self.reader, &mut probe)?;
        if extra > 0 {
            return Err(ConfinedIoError::Oversize {
                count: MAX.saturating_add(extra),
                max: MAX,
            }
            .into());
        }
        Ok(appended)
    }

    fn read_to_string(&mut self, buf: &mut String) -> io::Result<usize> {
        let mut raw = Vec::new();
        let read = self.read_to_end(&mut raw)?;
        let text =
            String::from_utf8(raw).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        if buf.is_empty() {
            *buf = text;
        } else {
            buf.push_str(&text);
        }
        Ok(read)
    }
}

impl<R: io::BufRead, const MIN: usize, const MAX: usize> io::BufRead
    for ConfinedReader<R, MIN, MAX>
{
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        fill_buf_confined::<R, MIN, MAX>(&mut self.reader, self.count)
    }

    fn consume(&mut self, amt: usize) {
        let amt = amt.min(MAX - self.count);
        self.count += amt;
        self.reader.consume(amt);
    }
}

#[cfg(any(unix, target_os = "wasi"))]
impl<R: AsFd, const MIN: usize, const MAX: usize> AsFd for ConfinedReader<R, MIN, MAX> {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.reader.as_fd()
    }
}

#[cfg(any(unix, target_os = "wasi"))]
impl<R: AsRawFd, const MIN: usize, const MAX: usize> AsRawFd for ConfinedReader<R, MIN, MAX> {
    fn as_raw_fd(&self) -> RawFd {
        self.reader.as_raw_fd()
    }
}

#[cfg(windows)]
impl<R: AsSocket, const MIN: usize, const MAX: usize> AsSocket for ConfinedReader<R, MIN, MAX> {
    fn as_socket(&self) -> BorrowedSocket<'_> {
        self.reader.as_socket()
    }
}

#[cfg(windows)]
impl<R: AsRawSocket, const MIN: usize, const MAX: usize> AsRawSocket
    for ConfinedReader<R, MIN, MAX>
{
    fn as_raw_socket(&self) -> RawSocket {
        self.reader.as_raw_socket()
    }
}

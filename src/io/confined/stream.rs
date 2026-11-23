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
use confined::{U8MAX, U16MAX, U24MAX, U32MAX, ZERO};

use super::error::{ConfinedIoError, UnfinishedIoError};
use super::reader::{ConfinedReader, fill_buf_confined};
use super::writer::ConfinedWriter;
use crate::io::{IoPair, JoinIoError, NullIo, SplitIo, SplitIoError};

/// Bidirectional stream wrapper enforcing independent compile-time confinement
/// bounds for read (`READ_MIN..=READ_MAX`) and write
/// (`WRITE_MIN..=WRITE_MAX`) directions.
///
/// When `WRITE_MIN` and `WRITE_MAX` are omitted, they default to `READ_MIN` and
/// `READ_MAX`, so `ConfinedIo<S, MIN, MAX>` expresses symmetric bounds while
/// `ConfinedIo<S, RMIN, RMAX, WMIN, WMAX>` expresses asymmetric read/write
/// bounds.
///
/// When `S` implements [`SplitIo`] (such as [`std::net::TcpStream`],
/// `UnixStream`, [`IoPair`], or `(R, W)`), `ConfinedIo` also implements
/// [`SplitIo`], splitting into
/// [`ConfinedReader<S::Reader, READ_MIN, READ_MAX>`] and
/// [`ConfinedWriter<S::Writer, WRITE_MIN, WRITE_MAX>`] while preserving exact
/// transferred byte counts and bounds across splits and rejoins.
///
/// # Examples
///
/// ```
/// use std::io::{Read, Write};
/// use amplify::io::{ConfinedIo, IoPair, SplitIo};
///
/// let (pipe_r, pipe_w) = std::io::pipe().unwrap();
/// let stream = ConfinedIo::<_, 1, 8, 1, 8>::from_pair(pipe_r, pipe_w);
/// let (mut r, mut w) = stream.split().unwrap();
///
/// w.write_all(b"ping").unwrap();
/// let mut buf = [0u8; 4];
/// r.read_exact(&mut buf).unwrap();
/// assert_eq!(&buf, b"ping");
///
/// let reunited: ConfinedIo<IoPair<_, _>, 1, 8, 1, 8> = SplitIo::join(r, w);
/// assert_eq!(reunited.counts(), (4, 4));
/// let _pair = reunited.try_release().unwrap();
/// ```
///
/// Inverted read or write bounds are rejected at build time:
///
/// ```compile_fail,E0080
/// use amplify::io::ConfinedIo;
///
/// let _ = ConfinedIo::<_, 0, 8, 5, 4>::null();
/// ```
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct ConfinedIo<
    S,
    const READ_MIN: usize,
    const READ_MAX: usize,
    const WRITE_MIN: usize = READ_MIN,
    const WRITE_MAX: usize = READ_MAX,
> {
    read_count: usize,
    write_count: usize,
    stream: S,
}

impl<
        S,
        const READ_MIN: usize,
        const READ_MAX: usize,
        const WRITE_MIN: usize,
        const WRITE_MAX: usize,
    > ConfinedIo<S, READ_MIN, READ_MAX, WRITE_MIN, WRITE_MAX>
{
    const ASSERT_BOUNDS: () = {
        assert!(
            READ_MIN <= READ_MAX,
            "the lower read bound of the stream confinement exceeds its upper bound"
        );
        assert!(
            WRITE_MIN <= WRITE_MAX,
            "the lower write bound of the stream confinement exceeds its upper bound"
        );
    };

    /// Wraps `stream` in a bidirectional confined stream.
    ///
    /// Fails to compile if `READ_MIN > READ_MAX` or `WRITE_MIN > WRITE_MAX`.
    #[inline]
    pub const fn new(stream: S) -> Self {
        let () = Self::ASSERT_BOUNDS;
        Self {
            read_count: 0,
            write_count: 0,
            stream,
        }
    }

    #[inline]
    pub(super) const fn from_raw_parts(stream: S, read_count: usize, write_count: usize) -> Self {
        let () = Self::ASSERT_BOUNDS;
        assert!(
            read_count <= READ_MAX,
            "confined stream read count exceeds upper read bound"
        );
        assert!(
            write_count <= WRITE_MAX,
            "confined stream write count exceeds upper write bound"
        );
        Self {
            read_count,
            write_count,
            stream,
        }
    }

    /// Returns the number of bytes read through this confined stream so far.
    #[inline]
    pub const fn read_count(&self) -> usize {
        self.read_count
    }

    /// Returns the number of bytes written through this confined stream so far.
    #[inline]
    pub const fn write_count(&self) -> usize {
        self.write_count
    }

    /// Returns `(read_count, write_count)`.
    #[inline]
    pub const fn counts(&self) -> (usize, usize) {
        (self.read_count, self.write_count)
    }

    /// Returns the number of bytes read as [`u8`].
    ///
    /// Fails to compile unless `READ_MAX <= U8MAX`.
    #[inline]
    pub const fn read_count_u8(&self) -> u8 {
        const {
            assert!(
                READ_MAX <= U8MAX,
                "`read_count_u8` is allowed only when `READ_MAX <= U8MAX`"
            );
        };
        self.read_count as u8
    }

    /// Returns the number of bytes read as [`u16`].
    ///
    /// Fails to compile unless `READ_MAX <= U16MAX`.
    #[inline]
    pub const fn read_count_u16(&self) -> u16 {
        const {
            assert!(
                READ_MAX <= U16MAX,
                "`read_count_u16` is allowed only when `READ_MAX <= U16MAX`"
            );
        };
        self.read_count as u16
    }

    /// Returns the number of bytes read as [`u24`].
    ///
    /// Fails to compile unless `READ_MAX <= U24MAX`.
    #[inline]
    pub const fn read_count_u24(&self) -> u24 {
        const {
            assert!(
                READ_MAX <= U24MAX,
                "`read_count_u24` is allowed only when `READ_MAX <= U24MAX`"
            );
        };
        u24::with(self.read_count as u32)
    }

    /// Returns the number of bytes read as [`u32`].
    ///
    /// Fails to compile unless `READ_MAX <= U32MAX`.
    #[inline]
    pub const fn read_count_u32(&self) -> u32 {
        const {
            assert!(
                READ_MAX <= U32MAX,
                "`read_count_u32` is allowed only when `READ_MAX <= U32MAX`"
            );
        };
        self.read_count as u32
    }

    /// Returns the number of bytes written as [`u8`].
    ///
    /// Fails to compile unless `WRITE_MAX <= U8MAX`.
    #[inline]
    pub const fn write_count_u8(&self) -> u8 {
        const {
            assert!(
                WRITE_MAX <= U8MAX,
                "`write_count_u8` is allowed only when `WRITE_MAX <= U8MAX`"
            );
        };
        self.write_count as u8
    }

    /// Returns the number of bytes written as [`u16`].
    ///
    /// Fails to compile unless `WRITE_MAX <= U16MAX`.
    #[inline]
    pub const fn write_count_u16(&self) -> u16 {
        const {
            assert!(
                WRITE_MAX <= U16MAX,
                "`write_count_u16` is allowed only when `WRITE_MAX <= U16MAX`"
            );
        };
        self.write_count as u16
    }

    /// Returns the number of bytes written as [`u24`].
    ///
    /// Fails to compile unless `WRITE_MAX <= U24MAX`.
    #[inline]
    pub const fn write_count_u24(&self) -> u24 {
        const {
            assert!(
                WRITE_MAX <= U24MAX,
                "`write_count_u24` is allowed only when `WRITE_MAX <= U24MAX`"
            );
        };
        u24::with(self.write_count as u32)
    }

    /// Returns the number of bytes written as [`u32`].
    ///
    /// Fails to compile unless `WRITE_MAX <= U32MAX`.
    #[inline]
    pub const fn write_count_u32(&self) -> u32 {
        const {
            assert!(
                WRITE_MAX <= U32MAX,
                "`write_count_u32` is allowed only when `WRITE_MAX <= U32MAX`"
            );
        };
        self.write_count as u32
    }

    /// Returns the lower read bound `READ_MIN`.
    #[inline]
    pub const fn read_min_bound(&self) -> usize {
        READ_MIN
    }

    /// Returns the upper read bound `READ_MAX`.
    #[inline]
    pub const fn read_max_bound(&self) -> usize {
        READ_MAX
    }

    /// Returns the lower write bound `WRITE_MIN`.
    #[inline]
    pub const fn write_min_bound(&self) -> usize {
        WRITE_MIN
    }

    /// Returns the upper write bound `WRITE_MAX`.
    #[inline]
    pub const fn write_max_bound(&self) -> usize {
        WRITE_MAX
    }

    /// Returns the maximum number of additional bytes that may still be read.
    #[inline]
    pub const fn read_remaining(&self) -> usize {
        READ_MAX - self.read_count
    }

    /// Returns the minimum number of additional bytes still required to satisfy
    /// `READ_MIN`.
    #[inline]
    pub const fn read_remaining_min(&self) -> usize {
        READ_MIN.saturating_sub(self.read_count)
    }

    /// Returns the maximum number of additional bytes that may still be
    /// written.
    #[inline]
    pub const fn write_remaining(&self) -> usize {
        WRITE_MAX - self.write_count
    }

    /// Returns the minimum number of additional bytes still required to satisfy
    /// `WRITE_MIN`.
    #[inline]
    pub const fn write_remaining_min(&self) -> usize {
        WRITE_MIN.saturating_sub(self.write_count)
    }

    /// Returns `true` if `read_count >= READ_MIN`.
    #[inline]
    pub const fn is_read_satisfied(&self) -> bool {
        self.read_count >= READ_MIN
    }

    /// Returns `true` if `write_count >= WRITE_MIN`.
    #[inline]
    pub const fn is_write_satisfied(&self) -> bool {
        self.write_count >= WRITE_MIN
    }

    /// Returns `true` if both `READ_MIN` and `WRITE_MIN` are satisfied.
    #[inline]
    pub const fn is_satisfied(&self) -> bool {
        self.is_read_satisfied() && self.is_write_satisfied()
    }

    /// Returns `true` if `read_count == READ_MAX`.
    #[inline]
    pub const fn is_read_exhausted(&self) -> bool {
        self.read_count == READ_MAX
    }

    /// Returns `true` if `write_count == WRITE_MAX`.
    #[inline]
    pub const fn is_write_exhausted(&self) -> bool {
        self.write_count == WRITE_MAX
    }

    /// Validates that `read_count` satisfies `READ_MIN..=READ_MAX`.
    #[inline]
    pub const fn check_read(&self) -> Result<(), ConfinedIoError> {
        if self.read_count < READ_MIN {
            return Err(ConfinedIoError::Undersize {
                count: self.read_count,
                min: READ_MIN,
            });
        }
        Ok(())
    }

    /// Validates that `write_count` satisfies `WRITE_MIN..=WRITE_MAX`.
    #[inline]
    pub const fn check_write(&self) -> Result<(), ConfinedIoError> {
        if self.write_count < WRITE_MIN {
            return Err(ConfinedIoError::Undersize {
                count: self.write_count,
                min: WRITE_MIN,
            });
        }
        Ok(())
    }

    /// Validates that both read and write byte counts satisfy their minimum
    /// bounds.
    #[inline]
    pub const fn check(&self) -> Result<(), ConfinedIoError> {
        if let Err(err) = self.check_read() {
            return Err(err);
        }
        self.check_write()
    }

    /// Widens read and write confinement bounds without runtime checks.
    ///
    /// Fails to compile unless `RMIN2 <= READ_MIN`, `READ_MAX <= RMAX2`,
    /// `WMIN2 <= WRITE_MIN`, and `WRITE_MAX <= WMAX2`.
    #[inline]
    pub fn widen<const RMIN2: usize, const RMAX2: usize, const WMIN2: usize, const WMAX2: usize>(
        self,
    ) -> ConfinedIo<S, RMIN2, RMAX2, WMIN2, WMAX2> {
        const {
            assert!(
                RMIN2 <= READ_MIN && READ_MAX <= RMAX2 && WMIN2 <= WRITE_MIN && WRITE_MAX <= WMAX2,
                "the new bounds of the confinement do not include the current bounds"
            );
        };
        ConfinedIo {
            read_count: self.read_count,
            write_count: self.write_count,
            stream: self.stream,
        }
    }

    /// Returns a shared reference to the underlying unconfined stream.
    #[inline]
    pub const fn as_unconfined(&self) -> &S {
        &self.stream
    }

    /// Clones and returns an unconfined copy of the underlying stream.
    #[inline]
    pub fn to_unconfined(&self) -> S
    where
        S: Clone,
    {
        self.stream.clone()
    }

    /// Releases the underlying stream after verifying both `READ_MIN` and
    /// `WRITE_MIN`, preserving `self` inside [`UnfinishedIoError`] otherwise.
    #[inline]
    pub fn try_release(self) -> Result<S, UnfinishedIoError<Self>> {
        if let Err(error) = self.check() {
            return Err(UnfinishedIoError {
                stream: self,
                error,
            });
        }
        Ok(self.stream)
    }

    /// Releases the underlying stream, panicking if either `READ_MIN` or
    /// `WRITE_MIN` is not satisfied.
    ///
    /// # Panics
    ///
    /// Panics if `self.read_count() < READ_MIN` or
    /// `self.write_count() < WRITE_MIN`.
    #[inline]
    pub fn release_checked(self) -> S {
        self.try_release()
            .expect("stream confinement lower bounds are not satisfied")
    }

    /// Unconditionally unwraps the underlying stream without checking whether
    /// `READ_MIN` and `WRITE_MIN` have been satisfied.
    #[inline]
    pub fn into_unconfined(self) -> S {
        self.stream
    }
}

impl<S, const READ_MAX: usize, const WRITE_MAX: usize>
    ConfinedIo<S, ZERO, READ_MAX, ZERO, WRITE_MAX>
{
    /// Infallibly releases the underlying stream when both `READ_MIN == 0` and
    /// `WRITE_MIN == 0`.
    ///
    /// Streams with a non-zero lower bound do not provide this method; use
    /// [`Self::try_release`], [`Self::release_checked`], or
    /// [`Self::into_unconfined`] instead:
    ///
    /// ```compile_fail,E0599
    /// use amplify::io::NonEmptyIo;
    ///
    /// let io = NonEmptyIo::<_, 8>::null();
    /// let _ = io.release();
    /// ```
    #[inline]
    pub fn release(self) -> S {
        self.stream
    }
}

impl<
        const READ_MIN: usize,
        const READ_MAX: usize,
        const WRITE_MIN: usize,
        const WRITE_MAX: usize,
    > ConfinedIo<NullIo, READ_MIN, READ_MAX, WRITE_MIN, WRITE_MAX>
{
    /// Creates a confined no-op bidirectional stream backed by [`NullIo`].
    #[inline]
    pub const fn null() -> Self {
        Self::new(NullIo)
    }
}

impl<
        R,
        W,
        const READ_MIN: usize,
        const READ_MAX: usize,
        const WRITE_MIN: usize,
        const WRITE_MAX: usize,
    > ConfinedIo<IoPair<R, W>, READ_MIN, READ_MAX, WRITE_MIN, WRITE_MAX>
{
    /// Wraps an independent `reader` and `writer` as a confined [`IoPair`].
    #[inline]
    pub const fn from_pair(reader: R, writer: W) -> Self {
        Self::new(IoPair::new(reader, writer))
    }

    /// Combines an existing [`ConfinedReader`] and [`ConfinedWriter`] into a
    /// bidirectional [`ConfinedIo`] over [`IoPair`], preserving transferred
    /// byte counts and confinement bounds.
    #[inline]
    pub fn from_confined_halves(
        reader: ConfinedReader<R, READ_MIN, READ_MAX>,
        writer: ConfinedWriter<W, WRITE_MIN, WRITE_MAX>,
    ) -> Self {
        Self::from_raw_parts(
            IoPair::new(reader.reader, writer.writer),
            reader.count,
            writer.count,
        )
    }

    /// Separates this [`ConfinedIo`] into its constituent [`ConfinedReader`]
    /// and [`ConfinedWriter`] halves.
    #[inline]
    pub fn into_confined_halves(
        self,
    ) -> (
        ConfinedReader<R, READ_MIN, READ_MAX>,
        ConfinedWriter<W, WRITE_MIN, WRITE_MAX>,
    ) {
        let (r, w) = self.stream.into_parts();
        (
            ConfinedReader::from_raw_parts(r, self.read_count),
            ConfinedWriter::from_raw_parts(w, self.write_count),
        )
    }
}

impl<
        S,
        const READ_MIN: usize,
        const READ_MAX: usize,
        const WRITE_MIN: usize,
        const WRITE_MAX: usize,
    > From<S> for ConfinedIo<S, READ_MIN, READ_MAX, WRITE_MIN, WRITE_MAX>
{
    #[inline]
    fn from(stream: S) -> Self {
        Self::new(stream)
    }
}

impl<
        S: Default,
        const READ_MIN: usize,
        const READ_MAX: usize,
        const WRITE_MIN: usize,
        const WRITE_MAX: usize,
    > Default for ConfinedIo<S, READ_MIN, READ_MAX, WRITE_MIN, WRITE_MAX>
{
    #[inline]
    fn default() -> Self {
        Self::new(S::default())
    }
}

impl<
        R,
        W,
        const READ_MIN: usize,
        const READ_MAX: usize,
        const WRITE_MIN: usize,
        const WRITE_MAX: usize,
    > From<ConfinedIo<IoPair<R, W>, READ_MIN, READ_MAX, WRITE_MIN, WRITE_MAX>>
    for IoPair<ConfinedReader<R, READ_MIN, READ_MAX>, ConfinedWriter<W, WRITE_MIN, WRITE_MAX>>
{
    #[inline]
    fn from(confined: ConfinedIo<IoPair<R, W>, READ_MIN, READ_MAX, WRITE_MIN, WRITE_MAX>) -> Self {
        let (r, w) = confined.into_confined_halves();
        IoPair::new(r, w)
    }
}

impl<
        S,
        const READ_MIN: usize,
        const READ_MAX: usize,
        const WRITE_MIN: usize,
        const WRITE_MAX: usize,
    > AsRef<S> for ConfinedIo<S, READ_MIN, READ_MAX, WRITE_MIN, WRITE_MAX>
{
    #[inline]
    fn as_ref(&self) -> &S {
        &self.stream
    }
}

impl<
        S: io::Read,
        const READ_MIN: usize,
        const READ_MAX: usize,
        const WRITE_MIN: usize,
        const WRITE_MAX: usize,
    > io::Read for ConfinedIo<S, READ_MIN, READ_MAX, WRITE_MIN, WRITE_MAX>
{
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let mut reader = ConfinedReader::<&mut S, READ_MIN, READ_MAX> {
            count: self.read_count,
            reader: &mut self.stream,
        };
        let res = reader.read(buf);
        self.read_count = reader.count;
        res
    }

    fn read_vectored(&mut self, bufs: &mut [io::IoSliceMut<'_>]) -> io::Result<usize> {
        let mut reader = ConfinedReader::<&mut S, READ_MIN, READ_MAX> {
            count: self.read_count,
            reader: &mut self.stream,
        };
        let res = reader.read_vectored(bufs);
        self.read_count = reader.count;
        res
    }

    fn read_exact(&mut self, buf: &mut [u8]) -> io::Result<()> {
        let mut reader = ConfinedReader::<&mut S, READ_MIN, READ_MAX> {
            count: self.read_count,
            reader: &mut self.stream,
        };
        let res = reader.read_exact(buf);
        self.read_count = reader.count;
        res
    }

    fn read_to_end(&mut self, buf: &mut Vec<u8>) -> io::Result<usize> {
        let mut reader = ConfinedReader::<&mut S, READ_MIN, READ_MAX> {
            count: self.read_count,
            reader: &mut self.stream,
        };
        let res = reader.read_to_end(buf);
        self.read_count = reader.count;
        res
    }

    fn read_to_string(&mut self, buf: &mut String) -> io::Result<usize> {
        let mut reader = ConfinedReader::<&mut S, READ_MIN, READ_MAX> {
            count: self.read_count,
            reader: &mut self.stream,
        };
        let res = reader.read_to_string(buf);
        self.read_count = reader.count;
        res
    }
}

impl<
        S: io::BufRead,
        const READ_MIN: usize,
        const READ_MAX: usize,
        const WRITE_MIN: usize,
        const WRITE_MAX: usize,
    > io::BufRead for ConfinedIo<S, READ_MIN, READ_MAX, WRITE_MIN, WRITE_MAX>
{
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        fill_buf_confined::<S, READ_MIN, READ_MAX>(&mut self.stream, self.read_count)
    }

    fn consume(&mut self, amt: usize) {
        let amt = amt.min(READ_MAX - self.read_count);
        self.read_count += amt;
        self.stream.consume(amt);
    }
}

impl<
        S: io::Write,
        const READ_MIN: usize,
        const READ_MAX: usize,
        const WRITE_MIN: usize,
        const WRITE_MAX: usize,
    > io::Write for ConfinedIo<S, READ_MIN, READ_MAX, WRITE_MIN, WRITE_MAX>
{
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut writer = ConfinedWriter::<&mut S, WRITE_MIN, WRITE_MAX> {
            count: self.write_count,
            writer: &mut self.stream,
        };
        let res = writer.write(buf);
        self.write_count = writer.count;
        res
    }

    fn write_vectored(&mut self, bufs: &[io::IoSlice<'_>]) -> io::Result<usize> {
        let mut writer = ConfinedWriter::<&mut S, WRITE_MIN, WRITE_MAX> {
            count: self.write_count,
            writer: &mut self.stream,
        };
        let res = writer.write_vectored(bufs);
        self.write_count = writer.count;
        res
    }

    #[inline]
    fn flush(&mut self) -> io::Result<()> {
        self.stream.flush()
    }
}

impl<
        S: SplitIo,
        const READ_MIN: usize,
        const READ_MAX: usize,
        const WRITE_MIN: usize,
        const WRITE_MAX: usize,
    > SplitIo for ConfinedIo<S, READ_MIN, READ_MAX, WRITE_MIN, WRITE_MAX>
{
    type Reader = ConfinedReader<S::Reader, READ_MIN, READ_MAX>;
    type Writer = ConfinedWriter<S::Writer, WRITE_MIN, WRITE_MAX>;

    fn split(self) -> Result<(Self::Reader, Self::Writer), SplitIoError<Self>> {
        let read_count = self.read_count;
        let write_count = self.write_count;
        match self.stream.split() {
            Ok((reader, writer)) => Ok((
                ConfinedReader::from_raw_parts(reader, read_count),
                ConfinedWriter::from_raw_parts(writer, write_count),
            )),
            Err(SplitIoError { original, error }) => Err(SplitIoError {
                original: Self::from_raw_parts(original, read_count, write_count),
                error,
            }),
        }
    }

    fn try_join(
        reader: Self::Reader,
        writer: Self::Writer,
    ) -> Result<Self, JoinIoError<Self::Reader, Self::Writer>> {
        let read_count = reader.count;
        let write_count = writer.count;
        match S::try_join(reader.reader, writer.writer) {
            Ok(stream) => Ok(Self::from_raw_parts(stream, read_count, write_count)),
            Err(JoinIoError {
                reader: inner_reader,
                writer: inner_writer,
            }) => Err(JoinIoError {
                reader: ConfinedReader::from_raw_parts(inner_reader, read_count),
                writer: ConfinedWriter::from_raw_parts(inner_writer, write_count),
            }),
        }
    }
}

#[cfg(any(unix, target_os = "wasi"))]
impl<
        S: AsFd,
        const READ_MIN: usize,
        const READ_MAX: usize,
        const WRITE_MIN: usize,
        const WRITE_MAX: usize,
    > AsFd for ConfinedIo<S, READ_MIN, READ_MAX, WRITE_MIN, WRITE_MAX>
{
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.stream.as_fd()
    }
}

#[cfg(any(unix, target_os = "wasi"))]
impl<
        S: AsRawFd,
        const READ_MIN: usize,
        const READ_MAX: usize,
        const WRITE_MIN: usize,
        const WRITE_MAX: usize,
    > AsRawFd for ConfinedIo<S, READ_MIN, READ_MAX, WRITE_MIN, WRITE_MAX>
{
    fn as_raw_fd(&self) -> RawFd {
        self.stream.as_raw_fd()
    }
}

#[cfg(windows)]
impl<
        S: AsSocket,
        const READ_MIN: usize,
        const READ_MAX: usize,
        const WRITE_MIN: usize,
        const WRITE_MAX: usize,
    > AsSocket for ConfinedIo<S, READ_MIN, READ_MAX, WRITE_MIN, WRITE_MAX>
{
    fn as_socket(&self) -> BorrowedSocket<'_> {
        self.stream.as_socket()
    }
}

#[cfg(windows)]
impl<
        S: AsRawSocket,
        const READ_MIN: usize,
        const READ_MAX: usize,
        const WRITE_MIN: usize,
        const WRITE_MAX: usize,
    > AsRawSocket for ConfinedIo<S, READ_MIN, READ_MAX, WRITE_MIN, WRITE_MAX>
{
    fn as_raw_socket(&self) -> RawSocket {
        self.stream.as_raw_socket()
    }
}

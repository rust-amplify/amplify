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

//! Byte-counting I/O adapters for read, write, and splittable bidirectional
//! streams.

use core::mem;
use std::io;
#[cfg(any(unix, target_os = "wasi"))]
use std::os::fd::{AsFd, AsRawFd, BorrowedFd, RawFd};
#[cfg(windows)]
use std::os::windows::io::{AsRawSocket, AsSocket, BorrowedSocket, RawSocket};

use crate::io::{IoPair, JoinIoError, SplitIo, SplitIoError};

/// Zero-sized no-op I/O endpoint yielding immediate EOF on reads (like
/// [`io::Empty`]) and discarding all written bytes (like [`io::Sink`]).
///
/// Unlike [`io::Empty`] and [`io::Sink`], `NullIo` implements [`PartialEq`],
/// [`Eq`], [`PartialOrd`], [`Ord`], [`Hash`], and [`SplitIo`], allowing it to
/// serve as the zero-overhead default handle for [`ReadCounter`],
/// [`WriteCounter`], and [`IoCounter`].
#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash, Default, Debug)]
pub struct NullIo;

impl io::Read for NullIo {
    #[inline]
    fn read(&mut self, _buf: &mut [u8]) -> io::Result<usize> {
        Ok(0)
    }

    #[inline]
    fn read_vectored(&mut self, _bufs: &mut [io::IoSliceMut<'_>]) -> io::Result<usize> {
        Ok(0)
    }
}

impl io::BufRead for NullIo {
    #[inline]
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        Ok(&[])
    }

    #[inline]
    fn consume(&mut self, _amt: usize) {}
}

impl io::Write for NullIo {
    #[inline]
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        Ok(buf.len())
    }

    #[inline]
    fn write_vectored(&mut self, bufs: &[io::IoSlice<'_>]) -> io::Result<usize> {
        let total = bufs
            .iter()
            .fold(0usize, |acc, slice| acc.saturating_add(slice.len()));
        Ok(total)
    }

    #[inline]
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl io::Seek for NullIo {
    #[inline]
    fn seek(&mut self, pos: io::SeekFrom) -> io::Result<u64> {
        let (io::SeekFrom::Start(_) | io::SeekFrom::End(0..) | io::SeekFrom::Current(0..)) = pos
        else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid seek to a negative position",
            ));
        };
        Ok(0)
    }
}

impl SplitIo for NullIo {
    type Reader = NullIo;
    type Writer = NullIo;

    #[inline]
    fn split(self) -> Result<(Self::Reader, Self::Writer), SplitIoError<Self>> {
        Ok((NullIo, NullIo))
    }

    #[inline]
    fn try_join(
        _reader: Self::Reader,
        _writer: Self::Writer,
    ) -> Result<Self, JoinIoError<Self::Reader, Self::Writer>> {
        Ok(NullIo)
    }
}

/// Counts the exact number of bytes read through [`io::Read`] and
/// [`io::BufRead`].
///
/// Wraps any reader `R`, defaulting to [`NullIo`]. Short reads advance the
/// counter only by the number of bytes actually transferred, failed reads leave
/// the counter unchanged, and arithmetic overflow of [`usize`] is reported as
/// [`io::ErrorKind::OutOfMemory`] rather than panicking or wrapping.
///
/// # Examples
///
/// ```
/// use std::io::{Cursor, Read};
/// use amplify::io::ReadCounter;
///
/// let mut counter = ReadCounter::with(Cursor::new(b"amplify"));
/// let mut buf = [0u8; 4];
/// counter.read_exact(&mut buf).unwrap();
/// assert_eq!(&buf, b"ampl");
/// assert_eq!(counter.count(), 4);
///
/// let mut rest = Vec::new();
/// counter.read_to_end(&mut rest).unwrap();
/// assert_eq!(counter.count(), 7);
/// ```
#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash, Debug)]
pub struct ReadCounter<R = NullIo> {
    /// Count of bytes which passed through this reader.
    pub count: usize,
    reader: R,
}

impl ReadCounter<NullIo> {
    /// Creates an empty [`ReadCounter`] backed by [`NullIo`].
    #[inline]
    pub const fn new() -> Self {
        Self {
            count: 0,
            reader: NullIo,
        }
    }
}

impl Default for ReadCounter<NullIo> {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl ReadCounter<io::Repeat> {
    /// Creates a [`ReadCounter`] over an infinite stream of zero bytes
    /// (`io::repeat(0)`).
    #[inline]
    pub fn zeros() -> Self {
        Self::repeat(0)
    }

    /// Creates a [`ReadCounter`] over an infinite stream repeating `byte`.
    #[inline]
    pub fn repeat(byte: u8) -> Self {
        Self::with(io::repeat(byte))
    }
}

impl<R> ReadCounter<R> {
    /// Wraps `reader` with the byte counter initialized to `0`.
    #[inline]
    pub const fn with(reader: R) -> Self {
        Self { count: 0, reader }
    }

    /// Wraps `reader` with the byte counter initialized to `count`.
    #[inline]
    pub const fn with_count(reader: R, count: usize) -> Self {
        Self { count, reader }
    }

    /// Returns the number of bytes read through this counter so far.
    #[inline]
    pub const fn count(&self) -> usize {
        self.count
    }

    /// Returns `true` if zero bytes have been read so far.
    #[inline]
    pub const fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Resets the byte count to `0`, returning the previous count.
    #[inline]
    pub fn reset(&mut self) -> usize {
        mem::replace(&mut self.count, 0)
    }

    /// Returns a shared reference to the underlying reader.
    #[inline]
    pub const fn as_reader(&self) -> &R {
        &self.reader
    }

    /// Returns a mutable reference to the underlying reader.
    ///
    /// Reading directly from the returned reference bypasses this counter.
    #[inline]
    pub fn as_reader_mut(&mut self) -> &mut R {
        &mut self.reader
    }

    /// Unwraps this counter, returning the underlying reader.
    #[inline]
    pub fn into_reader(self) -> R {
        self.reader
    }

    /// Consumes this counter, returning `(count, reader)`.
    #[inline]
    pub fn into_parts(self) -> (usize, R) {
        (self.count, self.reader)
    }
}

impl<R> From<R> for ReadCounter<R> {
    #[inline]
    fn from(reader: R) -> Self {
        Self::with(reader)
    }
}

impl<R> AsRef<R> for ReadCounter<R> {
    #[inline]
    fn as_ref(&self) -> &R {
        &self.reader
    }
}

impl<R: io::Read> io::Read for ReadCounter<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            let read = self.reader.read(buf)?;
            if read != 0 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "inner reader reported non-zero bytes read into an empty buffer",
                ));
            }
            return Ok(0);
        }
        let remaining = usize::MAX - self.count;
        if remaining == 0 {
            return Err(io::Error::new(
                io::ErrorKind::OutOfMemory,
                "read byte counter overflow",
            ));
        }
        let allowed = buf.len().min(remaining);
        let read = self.reader.read(&mut buf[..allowed])?;
        if read > allowed {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "inner reader reported more bytes than requested buffer slice",
            ));
        }
        self.count += read;
        Ok(read)
    }

    fn read_vectored(&mut self, bufs: &mut [io::IoSliceMut<'_>]) -> io::Result<usize> {
        let Some(first_idx) = bufs.iter().position(|s| !s.is_empty()) else {
            return self.read(&mut []);
        };
        let remaining = usize::MAX - self.count;
        if remaining == 0 {
            return Err(io::Error::new(
                io::ErrorKind::OutOfMemory,
                "read byte counter overflow",
            ));
        }
        let total = bufs[first_idx..]
            .iter()
            .fold(0usize, |acc, s| acc.saturating_add(s.len()));
        if total <= remaining {
            let read = self.reader.read_vectored(bufs)?;
            if read > total {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "inner reader reported more bytes than vectored buffer capacity",
                ));
            }
            self.count += read;
            return Ok(read);
        }
        self.read(&mut bufs[first_idx])
    }
}

impl<R: io::BufRead> io::BufRead for ReadCounter<R> {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        let buf = self.reader.fill_buf()?;
        if buf.is_empty() {
            return Ok(buf);
        }
        let remaining = usize::MAX - self.count;
        if remaining == 0 {
            return Err(io::Error::new(
                io::ErrorKind::OutOfMemory,
                "read byte counter overflow",
            ));
        }
        Ok(&buf[..buf.len().min(remaining)])
    }

    fn consume(&mut self, amt: usize) {
        let amt = amt.min(usize::MAX - self.count);
        self.count += amt;
        self.reader.consume(amt);
    }
}

#[cfg(any(unix, target_os = "wasi"))]
impl<R: AsFd> AsFd for ReadCounter<R> {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.reader.as_fd()
    }
}

#[cfg(any(unix, target_os = "wasi"))]
impl<R: AsRawFd> AsRawFd for ReadCounter<R> {
    fn as_raw_fd(&self) -> RawFd {
        self.reader.as_raw_fd()
    }
}

#[cfg(windows)]
impl<R: AsSocket> AsSocket for ReadCounter<R> {
    fn as_socket(&self) -> BorrowedSocket<'_> {
        self.reader.as_socket()
    }
}

#[cfg(windows)]
impl<R: AsRawSocket> AsRawSocket for ReadCounter<R> {
    fn as_raw_socket(&self) -> RawSocket {
        self.reader.as_raw_socket()
    }
}

/// Counts the exact number of bytes written through [`io::Write`].
///
/// By default (`W = `[`NullIo`]), `WriteCounter` discards written bytes while
/// recording their length, making [`WriteCounter::new`] /
/// [`WriteCounter::default`] a zero-allocation sizing sink. It can also wrap
/// any writer `W` via [`WriteCounter::with`] or [`From`], advancing the counter
/// only by the bytes actually accepted by `W`.
///
/// # Examples
///
/// ```
/// use std::io::Write;
/// use amplify::io::WriteCounter;
///
/// // Standalone sizing sink:
/// let mut sink_counter = WriteCounter::default();
/// sink_counter.write_all(b"hello").unwrap();
/// assert_eq!(sink_counter.count(), 5);
///
/// // Wrapping an actual writer:
/// let mut buf_counter = WriteCounter::with(Vec::new());
/// buf_counter.write_all(b"world").unwrap();
/// assert_eq!(buf_counter.count(), 5);
/// assert_eq!(buf_counter.into_writer(), b"world");
/// ```
#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash, Debug)]
pub struct WriteCounter<W = NullIo> {
    /// Count of bytes which passed through this writer.
    pub count: usize,
    writer: W,
}

impl WriteCounter<NullIo> {
    /// Creates a standalone byte-counting sink backed by [`NullIo`].
    #[inline]
    pub const fn new() -> Self {
        Self {
            count: 0,
            writer: NullIo,
        }
    }
}

impl Default for WriteCounter<NullIo> {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl<W> WriteCounter<W> {
    /// Wraps `writer` with the byte counter initialized to `0`.
    #[inline]
    pub const fn with(writer: W) -> Self {
        Self { count: 0, writer }
    }

    /// Wraps `writer` with the byte counter initialized to `count`.
    #[inline]
    pub const fn with_count(writer: W, count: usize) -> Self {
        Self { count, writer }
    }

    /// Returns the number of bytes written through this counter so far.
    #[inline]
    pub const fn count(&self) -> usize {
        self.count
    }

    /// Returns `true` if zero bytes have been written so far.
    #[inline]
    pub const fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Resets the byte count to `0`, returning the previous count.
    #[inline]
    pub fn reset(&mut self) -> usize {
        mem::replace(&mut self.count, 0)
    }

    /// Returns a shared reference to the underlying writer.
    #[inline]
    pub const fn as_writer(&self) -> &W {
        &self.writer
    }

    /// Returns a mutable reference to the underlying writer.
    ///
    /// Writing directly to the returned reference bypasses this counter.
    #[inline]
    pub fn as_writer_mut(&mut self) -> &mut W {
        &mut self.writer
    }

    /// Unwraps this counter, returning the underlying writer.
    #[inline]
    pub fn into_writer(self) -> W {
        self.writer
    }

    /// Consumes this counter, returning `(count, writer)`.
    #[inline]
    pub fn into_parts(self) -> (usize, W) {
        (self.count, self.writer)
    }
}

impl<W> From<W> for WriteCounter<W> {
    #[inline]
    fn from(writer: W) -> Self {
        Self::with(writer)
    }
}

impl<W> AsRef<W> for WriteCounter<W> {
    #[inline]
    fn as_ref(&self) -> &W {
        &self.writer
    }
}

impl<W: io::Write> io::Write for WriteCounter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if buf.is_empty() {
            let written = self.writer.write(buf)?;
            if written != 0 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "inner writer reported non-zero bytes written from an empty buffer",
                ));
            }
            return Ok(0);
        }
        let remaining = usize::MAX - self.count;
        if remaining == 0 {
            return Err(io::Error::new(
                io::ErrorKind::OutOfMemory,
                "write byte counter overflow",
            ));
        }
        let allowed = buf.len().min(remaining);
        let written = self.writer.write(&buf[..allowed])?;
        if written > allowed {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "inner writer reported more bytes than provided buffer slice",
            ));
        }
        self.count += written;
        Ok(written)
    }

    fn write_vectored(&mut self, bufs: &[io::IoSlice<'_>]) -> io::Result<usize> {
        let Some(first_idx) = bufs.iter().position(|s| !s.is_empty()) else {
            return self.write(&[]);
        };
        let remaining = usize::MAX - self.count;
        if remaining == 0 {
            return Err(io::Error::new(
                io::ErrorKind::OutOfMemory,
                "write byte counter overflow",
            ));
        }
        let total = bufs[first_idx..]
            .iter()
            .fold(0usize, |acc, s| acc.saturating_add(s.len()));
        if total <= remaining {
            let written = self.writer.write_vectored(bufs)?;
            if written > total {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "inner writer reported more bytes than vectored buffer length",
                ));
            }
            self.count += written;
            return Ok(written);
        }
        self.write(&bufs[first_idx])
    }

    #[inline]
    fn flush(&mut self) -> io::Result<()> {
        self.writer.flush()
    }
}

#[cfg(any(unix, target_os = "wasi"))]
impl<W: AsFd> AsFd for WriteCounter<W> {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.writer.as_fd()
    }
}

#[cfg(any(unix, target_os = "wasi"))]
impl<W: AsRawFd> AsRawFd for WriteCounter<W> {
    fn as_raw_fd(&self) -> RawFd {
        self.writer.as_raw_fd()
    }
}

#[cfg(windows)]
impl<W: AsSocket> AsSocket for WriteCounter<W> {
    fn as_socket(&self) -> BorrowedSocket<'_> {
        self.writer.as_socket()
    }
}

#[cfg(windows)]
impl<W: AsRawSocket> AsRawSocket for WriteCounter<W> {
    fn as_raw_socket(&self) -> RawSocket {
        self.writer.as_raw_socket()
    }
}

/// Combined bidirectional byte counter tracking both read and write byte totals
/// across a stream `S`.
///
/// When `S` implements [`SplitIo`] (such as [`std::net::TcpStream`],
/// `UnixStream`, [`IoPair`], or `(R, W)`), `IoCounter<S>` also implements
/// [`SplitIo`] with [`ReadCounter<S::Reader>`] and [`WriteCounter<S::Writer>`]
/// as its halves, preserving accumulated byte counts across splits and
/// reuniting them on [`SplitIo::try_join`] / [`SplitIo::join`].
///
/// # Examples
///
/// ```
/// use std::io::{Read, Write};
/// use amplify::io::{IoCounter, IoPair, SplitIo};
///
/// let (pipe_r, pipe_w) = std::io::pipe().unwrap();
/// let mut stream = IoCounter::from_pair(pipe_r, pipe_w);
///
/// stream.write_all(b"hi").unwrap();
/// let (mut reader, mut writer) = stream.split().unwrap();
/// writer.write_all(b"!").unwrap();
///
/// let mut buf = [0u8; 3];
/// reader.read_exact(&mut buf).unwrap();
/// assert_eq!(&buf, b"hi!");
///
/// let reunited = IoCounter::<IoPair<_, _>>::join(reader, writer);
/// assert_eq!(reunited.read_count(), 3);
/// assert_eq!(reunited.write_count(), 3);
/// ```
#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash, Debug)]
pub struct IoCounter<S = NullIo> {
    /// Count of bytes read through this stream.
    pub read_count: usize,
    /// Count of bytes written through this stream.
    pub write_count: usize,
    stream: S,
}

impl IoCounter<NullIo> {
    /// Creates a bidirectional byte counter backed by [`NullIo`].
    #[inline]
    pub const fn new() -> Self {
        Self {
            read_count: 0,
            write_count: 0,
            stream: NullIo,
        }
    }
}

impl Default for IoCounter<NullIo> {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl<R, W> IoCounter<IoPair<R, W>> {
    /// Wraps an independent `reader` and `writer` as a counted [`IoPair`].
    #[inline]
    pub const fn from_pair(reader: R, writer: W) -> Self {
        Self::with(IoPair::new(reader, writer))
    }

    /// Combines an existing [`ReadCounter`] and [`WriteCounter`] into a single
    /// [`IoCounter`] over [`IoPair`], preserving both counts.
    #[inline]
    pub fn from_counters(reader: ReadCounter<R>, writer: WriteCounter<W>) -> Self {
        let (read_count, reader) = reader.into_parts();
        let (write_count, writer) = writer.into_parts();
        Self::with_counts(IoPair::new(reader, writer), read_count, write_count)
    }

    /// Separates this [`IoCounter`] into its constituent [`ReadCounter`] and
    /// [`WriteCounter`] halves.
    #[inline]
    pub fn into_counters(self) -> (ReadCounter<R>, WriteCounter<W>) {
        let (read_count, write_count, pair) = self.into_parts();
        let (reader, writer) = pair.into_parts();
        (
            ReadCounter::with_count(reader, read_count),
            WriteCounter::with_count(writer, write_count),
        )
    }
}

impl<S> IoCounter<S> {
    /// Wraps `stream` with both read and write counters initialized to `0`.
    #[inline]
    pub const fn with(stream: S) -> Self {
        Self {
            read_count: 0,
            write_count: 0,
            stream,
        }
    }

    /// Wraps `stream` with explicit initial `read_count` and `write_count`.
    #[inline]
    pub const fn with_counts(stream: S, read_count: usize, write_count: usize) -> Self {
        Self {
            read_count,
            write_count,
            stream,
        }
    }

    /// Returns the number of bytes read through this stream so far.
    #[inline]
    pub const fn read_count(&self) -> usize {
        self.read_count
    }

    /// Returns the number of bytes written through this stream so far.
    #[inline]
    pub const fn write_count(&self) -> usize {
        self.write_count
    }

    /// Returns `(read_count, write_count)`.
    #[inline]
    pub const fn counts(&self) -> (usize, usize) {
        (self.read_count, self.write_count)
    }

    /// Returns `true` if neither reads nor writes have transferred any bytes.
    #[inline]
    pub const fn is_empty(&self) -> bool {
        self.read_count == 0 && self.write_count == 0
    }

    /// Resets both counters to `0`, returning the previous
    /// `(read_count, write_count)`.
    #[inline]
    pub fn reset(&mut self) -> (usize, usize) {
        (
            mem::replace(&mut self.read_count, 0),
            mem::replace(&mut self.write_count, 0),
        )
    }

    /// Resets only the read counter to `0`, returning its previous value.
    #[inline]
    pub fn reset_read(&mut self) -> usize {
        mem::replace(&mut self.read_count, 0)
    }

    /// Resets only the write counter to `0`, returning its previous value.
    #[inline]
    pub fn reset_write(&mut self) -> usize {
        mem::replace(&mut self.write_count, 0)
    }

    /// Returns a shared reference to the underlying stream.
    #[inline]
    pub const fn as_stream(&self) -> &S {
        &self.stream
    }

    /// Returns a mutable reference to the underlying stream.
    ///
    /// Performing I/O directly on the returned reference bypasses the counters.
    #[inline]
    pub fn as_stream_mut(&mut self) -> &mut S {
        &mut self.stream
    }

    /// Unwraps this counter, returning the underlying stream.
    #[inline]
    pub fn into_stream(self) -> S {
        self.stream
    }

    /// Consumes this counter, returning `(read_count, write_count, stream)`.
    #[inline]
    pub fn into_parts(self) -> (usize, usize, S) {
        (self.read_count, self.write_count, self.stream)
    }
}

impl<S> From<S> for IoCounter<S> {
    #[inline]
    fn from(stream: S) -> Self {
        Self::with(stream)
    }
}

impl<R, W> From<IoCounter<IoPair<R, W>>> for IoPair<ReadCounter<R>, WriteCounter<W>> {
    #[inline]
    fn from(counter: IoCounter<IoPair<R, W>>) -> Self {
        let (r, w) = counter.into_counters();
        IoPair::new(r, w)
    }
}

impl<S> AsRef<S> for IoCounter<S> {
    #[inline]
    fn as_ref(&self) -> &S {
        &self.stream
    }
}

impl<S: io::Read> io::Read for IoCounter<S> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let mut reader = ReadCounter {
            count: self.read_count,
            reader: &mut self.stream,
        };
        let res = reader.read(buf);
        self.read_count = reader.count;
        res
    }

    fn read_vectored(&mut self, bufs: &mut [io::IoSliceMut<'_>]) -> io::Result<usize> {
        let mut reader = ReadCounter {
            count: self.read_count,
            reader: &mut self.stream,
        };
        let res = reader.read_vectored(bufs);
        self.read_count = reader.count;
        res
    }
}

impl<S: io::BufRead> io::BufRead for IoCounter<S> {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        let count = self.read_count;
        let buf = self.stream.fill_buf()?;
        if buf.is_empty() {
            return Ok(buf);
        }
        let remaining = usize::MAX - count;
        if remaining == 0 {
            return Err(io::Error::new(
                io::ErrorKind::OutOfMemory,
                "read byte counter overflow",
            ));
        }
        Ok(&buf[..buf.len().min(remaining)])
    }

    fn consume(&mut self, amt: usize) {
        let amt = amt.min(usize::MAX - self.read_count);
        self.read_count += amt;
        self.stream.consume(amt);
    }
}

impl<S: io::Write> io::Write for IoCounter<S> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut writer = WriteCounter {
            count: self.write_count,
            writer: &mut self.stream,
        };
        let res = writer.write(buf);
        self.write_count = writer.count;
        res
    }

    fn write_vectored(&mut self, bufs: &[io::IoSlice<'_>]) -> io::Result<usize> {
        let mut writer = WriteCounter {
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

impl<S: SplitIo> SplitIo for IoCounter<S> {
    type Reader = ReadCounter<S::Reader>;
    type Writer = WriteCounter<S::Writer>;

    fn split(self) -> Result<(Self::Reader, Self::Writer), SplitIoError<Self>> {
        let read_count = self.read_count;
        let write_count = self.write_count;
        match self.stream.split() {
            Ok((reader, writer)) => Ok((
                ReadCounter::with_count(reader, read_count),
                WriteCounter::with_count(writer, write_count),
            )),
            Err(SplitIoError { original, error }) => Err(SplitIoError {
                original: Self::with_counts(original, read_count, write_count),
                error,
            }),
        }
    }

    fn try_join(
        reader: Self::Reader,
        writer: Self::Writer,
    ) -> Result<Self, JoinIoError<Self::Reader, Self::Writer>> {
        let (read_count, inner_reader) = reader.into_parts();
        let (write_count, inner_writer) = writer.into_parts();
        match S::try_join(inner_reader, inner_writer) {
            Ok(stream) => Ok(Self::with_counts(stream, read_count, write_count)),
            Err(JoinIoError {
                reader: inner_reader,
                writer: inner_writer,
            }) => Err(JoinIoError {
                reader: ReadCounter::with_count(inner_reader, read_count),
                writer: WriteCounter::with_count(inner_writer, write_count),
            }),
        }
    }
}

#[cfg(any(unix, target_os = "wasi"))]
impl<S: AsFd> AsFd for IoCounter<S> {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.stream.as_fd()
    }
}

#[cfg(any(unix, target_os = "wasi"))]
impl<S: AsRawFd> AsRawFd for IoCounter<S> {
    fn as_raw_fd(&self) -> RawFd {
        self.stream.as_raw_fd()
    }
}

#[cfg(windows)]
impl<S: AsSocket> AsSocket for IoCounter<S> {
    fn as_socket(&self) -> BorrowedSocket<'_> {
        self.stream.as_socket()
    }
}

#[cfg(windows)]
impl<S: AsRawSocket> AsRawSocket for IoCounter<S> {
    fn as_raw_socket(&self) -> RawSocket {
        self.stream.as_raw_socket()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::io::{BufRead, Cursor, IoSlice, IoSliceMut, Read, Seek, SeekFrom, Write};
    use std::net::{TcpListener, TcpStream};
    use std::thread;

    use super::*;

    struct ShortIo {
        data: Vec<u8>,
        read_pos: usize,
        written: Vec<u8>,
        fail_next: bool,
    }

    impl ShortIo {
        fn new(data: &[u8]) -> Self {
            Self {
                data: data.to_vec(),
                read_pos: 0,
                written: Vec::new(),
                fail_next: false,
            }
        }
    }

    impl Read for ShortIo {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            if self.fail_next {
                self.fail_next = false;
                return Err(io::Error::from(io::ErrorKind::Interrupted));
            }
            if buf.is_empty() || self.read_pos >= self.data.len() {
                return Ok(0);
            }
            let n = buf.len().min(2).min(self.data.len() - self.read_pos);
            buf[..n].copy_from_slice(&self.data[self.read_pos..self.read_pos + n]);
            self.read_pos += n;
            Ok(n)
        }
    }

    impl Write for ShortIo {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            if self.fail_next {
                self.fail_next = false;
                return Err(io::Error::from(io::ErrorKind::WouldBlock));
            }
            if buf.is_empty() {
                return Ok(0);
            }
            let n = buf.len().min(2);
            self.written.extend_from_slice(&buf[..n]);
            Ok(n)
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn null_io_semantics_and_split() {
        fn make_default<T: Default>() -> T {
            T::default()
        }
        let mut null: NullIo = make_default();
        let mut buf = [1u8; 4];
        assert_eq!(null.read(&mut buf).unwrap(), 0);
        assert_eq!(
            null.read_vectored(&mut [IoSliceMut::new(&mut buf)])
                .unwrap(),
            0
        );
        assert_eq!(null.fill_buf().unwrap(), b"");
        null.consume(0);
        assert_eq!(null.write(b"abcd").unwrap(), 4);
        assert_eq!(
            null.write_vectored(&[IoSlice::new(b"ab"), IoSlice::new(b"cde")])
                .unwrap(),
            5
        );
        null.flush().unwrap();
        assert_eq!(null.seek(SeekFrom::Start(10)).unwrap(), 0);
        assert_eq!(null.seek(SeekFrom::End(0)).unwrap(), 0);
        assert_eq!(null.stream_position().unwrap(), 0);
        assert!(null.seek(SeekFrom::Current(-1)).is_err());
        assert!(null.seek(SeekFrom::End(-1)).is_err());

        let (r, w) = SplitIo::split(null).unwrap();
        assert_eq!(NullIo::try_join(r, w).unwrap(), NullIo);
        assert_eq!(NullIo::join(r, w), NullIo);
    }

    #[test]
    fn write_counter_sink_and_wrapped_writer() {
        let mut counter = WriteCounter::default();
        assert!(counter.is_empty());
        counter.write_all(b"abc").unwrap();
        assert_eq!(counter.count, 3);
        assert_eq!(counter.count(), 3);
        assert!(!counter.is_empty());
        assert_eq!(counter.reset(), 3);
        assert_eq!(counter.count(), 0);

        let mut wrapped = WriteCounter::from(ShortIo::new(b""));
        assert_eq!(wrapped.write(b"").unwrap(), 0);
        assert_eq!(wrapped.write(b"hello").unwrap(), 2);
        assert_eq!(wrapped.count(), 2);
        assert_eq!(wrapped.as_writer().written, b"he");
        assert_eq!(wrapped.as_ref().written, b"he");
        wrapped.as_writer_mut().fail_next = true;
        assert_eq!(
            wrapped.write(b"llo").unwrap_err().kind(),
            io::ErrorKind::WouldBlock
        );
        assert_eq!(wrapped.count(), 2);
        wrapped.write_all(b"llo").unwrap();
        wrapped.flush().unwrap();
        assert_eq!(wrapped.count(), 5);
        assert_eq!(wrapped.into_writer().written, b"hello");
    }

    #[test]
    fn read_counter_empty_repeat_short_reads_and_bufread() {
        let mut empty = ReadCounter::default();
        let mut buf = [0u8; 4];
        assert_eq!(empty.read(&mut buf).unwrap(), 0);
        assert_eq!(empty.count(), 0);

        let mut zeros = ReadCounter::zeros();
        zeros.read_exact(&mut buf).unwrap();
        assert_eq!(buf, [0u8; 4]);
        assert_eq!(zeros.count(), 4);

        let mut short = ReadCounter::with(ShortIo::new(b"abcdef"));
        assert_eq!(short.read(&mut buf).unwrap(), 2);
        assert_eq!(short.count(), 2);
        short.as_reader_mut().fail_next = true;
        assert_eq!(
            short.read(&mut buf).unwrap_err().kind(),
            io::ErrorKind::Interrupted
        );
        assert_eq!(short.count(), 2);
        let mut rest = Vec::new();
        short.read_to_end(&mut rest).unwrap();
        assert_eq!(rest, b"cdef");
        assert_eq!(short.count(), 6);

        let mut buffered = ReadCounter::with(Cursor::new(b"first\nsecond".to_vec()));
        let mut line = String::new();
        buffered.read_line(&mut line).unwrap();
        assert_eq!(line, "first\n");
        assert_eq!(buffered.count(), 6);
    }

    #[test]
    fn vectored_and_overflow_checks_on_counters() {
        let mut r = ReadCounter::with_count(Cursor::new(b"abcd"), usize::MAX - 2);
        let mut a = [0u8; 2];
        let mut b = [0u8; 2];
        let n = r
            .read_vectored(&mut [IoSliceMut::new(&mut a), IoSliceMut::new(&mut b)])
            .unwrap();
        assert_eq!(n, 2);
        assert_eq!(&a, b"ab");
        assert_eq!(r.count(), usize::MAX);
        assert_eq!(r.read(&mut []).unwrap(), 0);
        assert_eq!(
            r.read(&mut a).unwrap_err().kind(),
            io::ErrorKind::OutOfMemory
        );

        let mut w = WriteCounter::with_count(Vec::new(), usize::MAX - 2);
        let n = w
            .write_vectored(&[IoSlice::new(b"xy"), IoSlice::new(b"zw")])
            .unwrap();
        assert_eq!(n, 2);
        assert_eq!(w.count(), usize::MAX);
        assert_eq!(w.write(&[]).unwrap(), 0);
        assert_eq!(
            w.write(b"z").unwrap_err().kind(),
            io::ErrorKind::OutOfMemory
        );
        assert_eq!(w.into_writer(), b"xy");
    }

    #[test]
    fn io_counter_tcp_split_rejoin_and_mismatched_join_preservation() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let client = TcpStream::connect(addr).unwrap();
        let (mut server, _) = listener.accept().unwrap();

        let mut counted = IoCounter::with(client);
        let server_thread = thread::spawn(move || {
            let mut req = [0u8; 7];
            server.read_exact(&mut req).unwrap();
            assert_eq!(&req, b"ab-ping");
            server.write_all(b"pong-xy").unwrap();
            server
        });

        counted.write_all(b"ab-").unwrap();
        assert_eq!(counted.write_count(), 3);

        let (mut reader, mut writer) = counted.split().unwrap();
        writer.write_all(b"ping").unwrap();
        writer.flush().unwrap();

        let mut resp = [0u8; 4];
        reader.read_exact(&mut resp).unwrap();
        assert_eq!(&resp, b"pong");

        let mut reunited = IoCounter::<TcpStream>::join(reader, writer);
        assert_eq!(reunited.read_count(), 4);
        assert_eq!(reunited.write_count(), 7);

        let mut tail = [0u8; 3];
        reunited.read_exact(&mut tail).unwrap();
        assert_eq!(&tail, b"-xy");
        assert_eq!(reunited.counts(), (7, 7));

        let _server = server_thread.join().unwrap();

        // Also test mismatched join preserves both counters!
        let listener2 = TcpListener::bind("127.0.0.1:0").unwrap();
        let c2 = TcpStream::connect(listener2.local_addr().unwrap()).unwrap();
        let (_s2, _) = listener2.accept().unwrap();

        let (r1, w1) = reunited.split().unwrap();
        let (r2, w2) = IoCounter::with_counts(c2, 11, 22).split().unwrap();

        let err = IoCounter::<TcpStream>::try_join(r1, w2).unwrap_err();
        assert_eq!(err.reader.count(), 7);
        assert_eq!(err.writer.count(), 22);

        let ok1 = IoCounter::<TcpStream>::join(err.reader, w1);
        let ok2 = IoCounter::<TcpStream>::join(r2, err.writer);
        assert_eq!(ok1.counts(), (7, 7));
        assert_eq!(ok2.counts(), (11, 22));
    }

    #[derive(Debug)]
    struct FaultyIo {
        buf: Vec<u8>,
        report_read: Option<usize>,
        report_write: Option<usize>,
        fail_kind: Option<io::ErrorKind>,
        fail_split: bool,
    }

    impl FaultyIo {
        fn new(data: &[u8]) -> Self {
            Self {
                buf: data.to_vec(),
                report_read: None,
                report_write: None,
                fail_kind: None,
                fail_split: false,
            }
        }
    }

    impl Read for FaultyIo {
        fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
            if let Some(kind) = self.fail_kind.take() {
                return Err(io::Error::from(kind));
            }
            if let Some(n) = self.report_read.take() {
                return Ok(n);
            }
            let n = out.len().min(self.buf.len());
            out[..n].copy_from_slice(&self.buf[..n]);
            self.buf.drain(..n);
            Ok(n)
        }

        fn read_vectored(&mut self, bufs: &mut [IoSliceMut<'_>]) -> io::Result<usize> {
            if let Some(kind) = self.fail_kind.take() {
                return Err(io::Error::from(kind));
            }
            if let Some(n) = self.report_read.take() {
                return Ok(n);
            }
            let mut total = 0;
            for slice in bufs {
                let n = slice.len().min(self.buf.len());
                slice[..n].copy_from_slice(&self.buf[..n]);
                self.buf.drain(..n);
                total += n;
            }
            Ok(total)
        }
    }

    impl BufRead for FaultyIo {
        fn fill_buf(&mut self) -> io::Result<&[u8]> {
            if let Some(kind) = self.fail_kind.take() {
                return Err(io::Error::from(kind));
            }
            Ok(&self.buf)
        }

        fn consume(&mut self, amt: usize) {
            let n = amt.min(self.buf.len());
            self.buf.drain(..n);
        }
    }

    impl Write for FaultyIo {
        fn write(&mut self, data: &[u8]) -> io::Result<usize> {
            if let Some(kind) = self.fail_kind.take() {
                return Err(io::Error::from(kind));
            }
            if let Some(n) = self.report_write.take() {
                return Ok(n);
            }
            self.buf.extend_from_slice(data);
            Ok(data.len())
        }

        fn write_vectored(&mut self, bufs: &[IoSlice<'_>]) -> io::Result<usize> {
            if let Some(kind) = self.fail_kind.take() {
                return Err(io::Error::from(kind));
            }
            if let Some(n) = self.report_write.take() {
                return Ok(n);
            }
            let mut total = 0;
            for slice in bufs {
                self.buf.extend_from_slice(slice);
                total += slice.len();
            }
            Ok(total)
        }

        fn flush(&mut self) -> io::Result<()> {
            if let Some(kind) = self.fail_kind.take() {
                return Err(io::Error::from(kind));
            }
            Ok(())
        }
    }

    impl SplitIo for FaultyIo {
        type Reader = FaultyIo;
        type Writer = FaultyIo;

        fn split(self) -> Result<(Self::Reader, Self::Writer), SplitIoError<Self>> {
            if self.fail_split {
                return Err(SplitIoError {
                    original: self,
                    error: io::Error::from(io::ErrorKind::BrokenPipe),
                });
            }
            Ok((Self::new(&self.buf), Self::new(&[])))
        }

        fn try_join(
            reader: Self::Reader,
            writer: Self::Writer,
        ) -> Result<Self, JoinIoError<Self::Reader, Self::Writer>> {
            let mut merged = reader.buf;
            merged.extend_from_slice(&writer.buf);
            Ok(Self::new(&merged))
        }
    }

    #[test]
    fn read_and_write_counter_accessors_malformed_inner_and_vectored_edges() {
        let mut rc = ReadCounter::from(FaultyIo::new(b"abcdef"));
        assert!(rc.is_empty());
        assert_eq!(rc.as_reader().buf, b"abcdef");
        assert_eq!(rc.as_ref().buf, b"abcdef");

        // Empty read ok, error, and non-zero-on-empty-buffer violation on
        // ReadCounter<FaultyIo>.
        assert_eq!(rc.read(&mut []).unwrap(), 0);
        rc.as_reader_mut().fail_kind = Some(io::ErrorKind::Other);
        assert_eq!(rc.read(&mut []).unwrap_err().kind(), io::ErrorKind::Other);
        rc.as_reader_mut().report_read = Some(1);
        assert_eq!(
            rc.read(&mut []).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );

        // Over-reporting, error, overflow, and normal read + read_vectored on
        // ReadCounter<FaultyIo>.
        let mut two = [0u8; 2];
        rc.as_reader_mut().fail_kind = Some(io::ErrorKind::Interrupted);
        assert_eq!(
            rc.read(&mut two).unwrap_err().kind(),
            io::ErrorKind::Interrupted
        );
        rc.as_reader_mut().report_read = Some(5);
        assert_eq!(
            rc.read(&mut two).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
        assert_eq!(rc.read(&mut two).unwrap(), 2);
        assert_eq!(&two, b"ab");

        rc.as_reader_mut().report_read = Some(5);
        assert_eq!(
            rc.read_vectored(&mut [IoSliceMut::new(&mut two)])
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidData
        );
        rc.as_reader_mut().fail_kind = Some(io::ErrorKind::TimedOut);
        assert_eq!(
            rc.read_vectored(&mut [IoSliceMut::new(&mut two)])
                .unwrap_err()
                .kind(),
            io::ErrorKind::TimedOut
        );

        // Normal vectored read, empty vectored read, and near-MAX fallback on
        // ReadCounter<FaultyIo>.
        assert_eq!(rc.read_vectored(&mut []).unwrap(), 0);
        let mut second = [0u8; 2];
        assert_eq!(
            rc.read_vectored(&mut [IoSliceMut::new(&mut two), IoSliceMut::new(&mut second)])
                .unwrap(),
            4
        );
        assert!(!rc.is_empty());
        assert_eq!(rc.reset(), 6);
        assert!(rc.is_empty());

        rc.as_reader_mut().buf.extend_from_slice(b"123");
        rc.count = usize::MAX - 1;
        assert_eq!(
            rc.read_vectored(&mut [IoSliceMut::new(&mut two)]).unwrap(),
            1
        );
        assert_eq!(
            rc.read(&mut two).unwrap_err().kind(),
            io::ErrorKind::OutOfMemory
        );

        // BufRead: error, normal slice capping near usize::MAX, empty EOF,
        // overflow.
        let mut brc = ReadCounter::with_count(FaultyIo::new(b"xyz"), usize::MAX - 2);
        brc.as_reader_mut().fail_kind = Some(io::ErrorKind::Interrupted);
        assert_eq!(
            brc.fill_buf().unwrap_err().kind(),
            io::ErrorKind::Interrupted
        );
        assert_eq!(brc.fill_buf().unwrap(), b"xy");
        brc.consume(5);
        assert_eq!(brc.count(), usize::MAX);
        assert_eq!(
            brc.fill_buf().unwrap_err().kind(),
            io::ErrorKind::OutOfMemory
        );
        assert_eq!(
            brc.read_vectored(&mut [IoSliceMut::new(&mut two)])
                .unwrap_err()
                .kind(),
            io::ErrorKind::OutOfMemory
        );
        let inner = brc.into_reader();
        assert_eq!(inner.buf, b"z");

        let mut empty_brc = ReadCounter::with_count(FaultyIo::new(b""), usize::MAX);
        assert_eq!(empty_brc.fill_buf().unwrap(), b"");

        // WriteCounter: all branches on WriteCounter<FaultyIo>.
        let mut wc = WriteCounter::with(FaultyIo::new(b""));
        assert_eq!(wc.write(b"").unwrap(), 0);
        wc.as_writer_mut().fail_kind = Some(io::ErrorKind::Other);
        assert_eq!(wc.write(b"").unwrap_err().kind(), io::ErrorKind::Other);
        wc.as_writer_mut().report_write = Some(1);
        assert_eq!(
            wc.write(b"").unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
        wc.as_writer_mut().fail_kind = Some(io::ErrorKind::WouldBlock);
        assert_eq!(
            wc.write(b"ab").unwrap_err().kind(),
            io::ErrorKind::WouldBlock
        );
        wc.as_writer_mut().report_write = Some(10);
        assert_eq!(
            wc.write(b"ab").unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
        assert_eq!(wc.write(b"x").unwrap(), 1);
        assert_eq!(wc.reset(), 1);
        wc.as_writer_mut().buf.clear();
        wc.as_writer_mut().report_write = Some(10);
        assert_eq!(
            wc.write_vectored(&[IoSlice::new(b"ab")])
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidData
        );
        wc.as_writer_mut().fail_kind = Some(io::ErrorKind::BrokenPipe);
        assert_eq!(
            wc.write_vectored(&[IoSlice::new(b"ab")])
                .unwrap_err()
                .kind(),
            io::ErrorKind::BrokenPipe
        );
        wc.as_writer_mut().fail_kind = Some(io::ErrorKind::Other);
        assert_eq!(wc.flush().unwrap_err().kind(), io::ErrorKind::Other);
        wc.flush().unwrap();

        assert_eq!(wc.write_vectored(&[]).unwrap(), 0);
        assert_eq!(
            wc.write_vectored(&[IoSlice::new(b"ab"), IoSlice::new(b"cd")])
                .unwrap(),
            4
        );
        let (wcount, w_inner) = wc.into_parts();
        assert_eq!(wcount, 4);
        assert_eq!(w_inner.buf, b"abcd");

        let mut max_wc = WriteCounter::with_count(FaultyIo::new(b""), usize::MAX - 1);
        assert_eq!(max_wc.write_vectored(&[IoSlice::new(b"ab")]).unwrap(), 1);
        assert_eq!(
            max_wc.write(b"a").unwrap_err().kind(),
            io::ErrorKind::OutOfMemory
        );
        assert_eq!(
            max_wc
                .write_vectored(&[IoSlice::new(b"a")])
                .unwrap_err()
                .kind(),
            io::ErrorKind::OutOfMemory
        );
    }

    #[test]
    fn io_counter_all_methods_bufread_split_error_and_fd_forwarding() {
        let mut noop = IoCounter::default();
        assert!(noop.is_empty());
        assert_eq!(noop.write(b"hi").unwrap(), 2);
        assert!(!noop.is_empty());
        assert_eq!(noop.reset_write(), 2);
        assert!(noop.is_empty());

        let r_part = ReadCounter::with_count(Cursor::new(b"hello\nworld".to_vec()), 3);
        let w_part = WriteCounter::with_count(Vec::<u8>::new(), 5);
        let mut combined = IoCounter::from_counters(r_part, w_part);
        assert_eq!(combined.counts(), (3, 5));
        assert!(!combined.is_empty());
        assert_eq!(combined.reset_read(), 3);
        assert_eq!(combined.counts(), (0, 5));
        assert_eq!(combined.reset(), (0, 5));
        assert!(combined.is_empty());

        let mut line = String::new();
        combined.read_line(&mut line).unwrap();
        assert_eq!(line, "hello\n");
        let mut v1 = [0u8; 2];
        let mut v2 = [0u8; 3];
        assert_eq!(
            combined
                .read_vectored(&mut [IoSliceMut::new(&mut v1), IoSliceMut::new(&mut v2)])
                .unwrap(),
            5
        );
        assert_eq!(&v1, b"wo");
        assert_eq!(&v2, b"rld");
        assert_eq!(combined.fill_buf().unwrap(), b"");

        assert_eq!(
            combined
                .write_vectored(&[IoSlice::new(b"aa"), IoSlice::new(b"bbb")])
                .unwrap(),
            5
        );
        combined.flush().unwrap();
        assert_eq!(combined.as_stream().as_writer(), b"aabbb");
        assert_eq!(combined.as_ref().as_writer(), b"aabbb");
        combined.as_stream_mut().as_writer_mut().push(b'!');

        let pair: IoPair<ReadCounter<_>, WriteCounter<_>> = combined.into();
        let (rc, wc) = pair.into_parts();
        assert_eq!(rc.count(), 11);
        assert_eq!(wc.count(), 5);
        let rebuilt = IoCounter::from_counters(rc, wc);
        let (rc2, wc2) = rebuilt.into_counters();
        assert_eq!(rc2.count(), 11);
        assert_eq!(wc2.into_writer(), b"aabbb!");

        // IoCounter BufRead overflow & error + split failure preservation.
        let mut faulty = IoCounter::from(FaultyIo::new(b"abcd"));
        faulty.read_count = usize::MAX - 1;
        faulty.write_count = 9;
        faulty.as_stream_mut().fail_kind = Some(io::ErrorKind::Other);
        assert_eq!(faulty.fill_buf().unwrap_err().kind(), io::ErrorKind::Other);
        assert_eq!(faulty.fill_buf().unwrap(), b"a");
        faulty.consume(1);
        assert_eq!(faulty.read_count(), usize::MAX);
        assert_eq!(
            faulty.fill_buf().unwrap_err().kind(),
            io::ErrorKind::OutOfMemory
        );

        faulty.as_stream_mut().fail_split = true;
        let split_err = SplitIo::split(faulty).unwrap_err();
        assert_eq!(split_err.error.kind(), io::ErrorKind::BrokenPipe);
        assert_eq!(split_err.original.counts(), (usize::MAX, 9));
        let mut recovered = split_err.original;
        recovered.as_stream_mut().buf.clear();
        assert_eq!(recovered.fill_buf().unwrap(), b"");
        recovered.as_stream_mut().buf.extend_from_slice(b"bcd");
        recovered.as_stream_mut().fail_split = false;
        let (sh_r, sh_w) = SplitIo::split(recovered).unwrap();
        let rejoined = IoCounter::<FaultyIo>::join(sh_r, sh_w);
        assert_eq!(rejoined.into_stream().buf, b"bcd");

        #[cfg(any(unix, target_os = "wasi"))]
        {
            let (pr, pw) = io::pipe().unwrap();
            let rc_fd = ReadCounter::with(pr);
            let wc_fd = WriteCounter::with(pw);
            assert!(rc_fd.as_fd().as_raw_fd() >= 0);
            assert_eq!(rc_fd.as_raw_fd(), rc_fd.as_fd().as_raw_fd());
            assert!(wc_fd.as_fd().as_raw_fd() >= 0);
            assert_eq!(wc_fd.as_raw_fd(), wc_fd.as_fd().as_raw_fd());

            let (u1, _u2) = std::os::unix::net::UnixStream::pair().unwrap();
            let ic_fd = IoCounter::with(u1);
            assert!(ic_fd.as_fd().as_raw_fd() >= 0);
            assert_eq!(ic_fd.as_raw_fd(), ic_fd.as_fd().as_raw_fd());
        }

        // Exercise derived Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug
        // on all counter types.
        let n1 = NullIo;
        let n2 = n1;
        assert_eq!(n1, n2);
        assert_eq!(n1.cmp(&n2), core::cmp::Ordering::Equal);
        assert_eq!(n1.partial_cmp(&n2), Some(core::cmp::Ordering::Equal));
        assert!(!format!("{n1:?}").is_empty());
        let mut nset = HashSet::new();
        assert!(nset.insert(n1));

        let r1 = ReadCounter::with_count(vec![10u8], 1);
        let r2 = r1.clone();
        let r3 = ReadCounter::with_count(vec![10u8], 2);
        let r4 = ReadCounter::with_count(vec![20u8], 1);
        assert_eq!(r1, r2);
        assert_ne!(r1, r3);
        assert_ne!(r1, r4);
        assert!(r1 < r3);
        assert!(r1 < r4);
        assert_eq!(r1.cmp(&r2), core::cmp::Ordering::Equal);
        assert!(!format!("{r1:?}").is_empty());
        let mut rset = HashSet::new();
        assert!(rset.insert(r1));

        let w1 = WriteCounter::with_count(vec![10u8], 1);
        let w2 = w1.clone();
        let w3 = WriteCounter::with_count(vec![10u8], 2);
        let w4 = WriteCounter::with_count(vec![20u8], 1);
        assert_eq!(w1, w2);
        assert_ne!(w1, w3);
        assert_ne!(w1, w4);
        assert!(w1 < w3);
        assert!(w1 < w4);
        assert_eq!(w1.cmp(&w2), core::cmp::Ordering::Equal);
        assert!(!format!("{w1:?}").is_empty());
        let mut wset = HashSet::new();
        assert!(wset.insert(w1));

        let i1 = IoCounter::with_counts(vec![10u8], 1, 2);
        let i2 = i1.clone();
        let i3 = IoCounter::with_counts(vec![10u8], 2, 2);
        let i4 = IoCounter::with_counts(vec![10u8], 1, 3);
        let i5 = IoCounter::with_counts(vec![20u8], 1, 2);
        assert_eq!(i1, i2);
        assert_ne!(i1, i3);
        assert_ne!(i1, i4);
        assert_ne!(i1, i5);
        assert!(i1 < i3);
        assert!(i1 < i4);
        assert!(i1 < i5);
        assert_eq!(i1.cmp(&i2), core::cmp::Ordering::Equal);
        assert!(!format!("{i1:?}").is_empty());
        let mut iset = HashSet::new();
        assert!(iset.insert(i1));
    }
}

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
use std::net::TcpStream;
#[cfg(any(unix, target_os = "wasi"))]
use std::os::fd::{AsFd, AsRawFd, BorrowedFd, RawFd};
#[cfg(unix)]
use std::os::unix::net::UnixStream;
#[cfg(windows)]
use std::os::windows::io::{AsRawSocket, AsSocket, BorrowedSocket, RawSocket};
#[cfg(not(target_has_atomic = "64"))]
use std::sync::Mutex;
#[cfg(target_has_atomic = "64")]
use std::sync::atomic::{AtomicU64, Ordering};

#[cfg(target_has_atomic = "64")]
fn next_split_id() -> u64 {
    static NEXT_SPLIT_ID: AtomicU64 = AtomicU64::new(1);
    NEXT_SPLIT_ID.fetch_add(1, Ordering::Relaxed)
}

#[cfg(not(target_has_atomic = "64"))]
fn next_split_id() -> u64 {
    static NEXT_SPLIT_ID: Mutex<u64> = Mutex::new(1);
    let mut guard = NEXT_SPLIT_ID.lock().unwrap_or_else(|e| e.into_inner());
    let id = *guard;
    *guard = id.wrapping_add(1);
    id
}

/// Error returned by [`SplitIo::split`] when an I/O stream cannot be separated
/// into reader and writer halves, preserving the original value.
#[derive(Debug)]
pub struct SplitIoError<T> {
    /// The original I/O value that could not be split.
    pub original: T,
    /// The underlying I/O error.
    pub error: io::Error,
}

impl<T> fmt::Display for SplitIoError<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.error, f)
    }
}

impl<T: fmt::Debug> StdError for SplitIoError<T> {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        Some(&self.error)
    }
}

impl<T> From<SplitIoError<T>> for io::Error {
    fn from(err: SplitIoError<T>) -> Self {
        err.error
    }
}

/// Error returned by [`SplitIo::try_join`] when attempting to reunite reader
/// and writer halves that did not originate from the same [`SplitIo::split`]
/// call.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct JoinIoError<R, W> {
    /// The reader half passed to [`SplitIo::try_join`].
    pub reader: R,
    /// The writer half passed to [`SplitIo::try_join`].
    pub writer: W,
}

impl<R, W> fmt::Display for JoinIoError<R, W> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("cannot join I/O reader and writer halves originating from different streams")
    }
}

impl<R: fmt::Debug, W: fmt::Debug> StdError for JoinIoError<R, W> {}

impl<R, W> From<JoinIoError<R, W>> for io::Error {
    fn from(_: JoinIoError<R, W>) -> Self {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "cannot join I/O reader and writer halves originating from different streams",
        )
    }
}

/// Trait for bidirectional I/O streams and paired handles that can be separated
/// into independent [`SplitIo::Reader`] and [`SplitIo::Writer`] halves and
/// joined back into the original type.
pub trait SplitIo: Sized {
    /// Reader half produced by [`SplitIo::split`].
    type Reader: io::Read;
    /// Writer half produced by [`SplitIo::split`].
    type Writer: io::Write;

    /// Splits `self` into independent reader and writer halves.
    ///
    /// On failure (such as when duplicating an OS socket descriptor fails),
    /// returns [`SplitIoError`] containing both the original value and the
    /// underlying [`io::Error`].
    fn split(self) -> Result<(Self::Reader, Self::Writer), SplitIoError<Self>>;

    /// Attempts to reconstruct `self` from its reader and writer halves.
    ///
    /// Returns [`JoinIoError`] preserving both halves if they did not originate
    /// from the same [`SplitIo::split`] call.
    fn try_join(
        reader: Self::Reader,
        writer: Self::Writer,
    ) -> Result<Self, JoinIoError<Self::Reader, Self::Writer>>;

    /// Reconstructs `self` from its reader and writer halves.
    ///
    /// # Panics
    ///
    /// Panics if `reader` and `writer` did not originate from the same
    /// [`SplitIo::split`] call.
    fn join(reader: Self::Reader, writer: Self::Writer) -> Self {
        match Self::try_join(reader, writer) {
            Ok(joined) => joined,
            Err(_) => {
                panic!("joining I/O halves which were not produced by the same split() call")
            }
        }
    }
}

/// Reader half of a split stream such as [`TcpStream`] or `UnixStream`.
#[derive(Debug)]
pub struct StreamReader<S> {
    id: u64,
    stream: S,
}

impl<S> StreamReader<S> {
    /// Returns a shared reference to the underlying stream handle.
    pub fn as_stream(&self) -> &S {
        &self.stream
    }
}

impl<S> AsRef<S> for StreamReader<S> {
    fn as_ref(&self) -> &S {
        &self.stream
    }
}

impl<S: io::Read> io::Read for StreamReader<S> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.stream.read(buf)
    }

    fn read_vectored(&mut self, bufs: &mut [io::IoSliceMut<'_>]) -> io::Result<usize> {
        self.stream.read_vectored(bufs)
    }

    fn read_to_end(&mut self, buf: &mut Vec<u8>) -> io::Result<usize> {
        self.stream.read_to_end(buf)
    }

    fn read_to_string(&mut self, buf: &mut String) -> io::Result<usize> {
        self.stream.read_to_string(buf)
    }

    fn read_exact(&mut self, buf: &mut [u8]) -> io::Result<()> {
        self.stream.read_exact(buf)
    }
}

#[cfg(any(unix, target_os = "wasi"))]
impl<S: AsFd> AsFd for StreamReader<S> {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.stream.as_fd()
    }
}

#[cfg(any(unix, target_os = "wasi"))]
impl<S: AsRawFd> AsRawFd for StreamReader<S> {
    fn as_raw_fd(&self) -> RawFd {
        self.stream.as_raw_fd()
    }
}

#[cfg(windows)]
impl<S: AsSocket> AsSocket for StreamReader<S> {
    fn as_socket(&self) -> BorrowedSocket<'_> {
        self.stream.as_socket()
    }
}

#[cfg(windows)]
impl<S: AsRawSocket> AsRawSocket for StreamReader<S> {
    fn as_raw_socket(&self) -> RawSocket {
        self.stream.as_raw_socket()
    }
}

/// Writer half of a split stream such as [`TcpStream`] or `UnixStream`.
#[derive(Debug)]
pub struct StreamWriter<S> {
    id: u64,
    stream: S,
}

impl<S> StreamWriter<S> {
    /// Returns a shared reference to the underlying stream handle.
    pub fn as_stream(&self) -> &S {
        &self.stream
    }
}

impl<S> AsRef<S> for StreamWriter<S> {
    fn as_ref(&self) -> &S {
        &self.stream
    }
}

impl<S: io::Write> io::Write for StreamWriter<S> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.stream.write(buf)
    }

    fn write_vectored(&mut self, bufs: &[io::IoSlice<'_>]) -> io::Result<usize> {
        self.stream.write_vectored(bufs)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.stream.flush()
    }

    fn write_all(&mut self, buf: &[u8]) -> io::Result<()> {
        self.stream.write_all(buf)
    }

    fn write_fmt(&mut self, args: fmt::Arguments<'_>) -> io::Result<()> {
        self.stream.write_fmt(args)
    }
}

#[cfg(any(unix, target_os = "wasi"))]
impl<S: AsFd> AsFd for StreamWriter<S> {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.stream.as_fd()
    }
}

#[cfg(any(unix, target_os = "wasi"))]
impl<S: AsRawFd> AsRawFd for StreamWriter<S> {
    fn as_raw_fd(&self) -> RawFd {
        self.stream.as_raw_fd()
    }
}

#[cfg(windows)]
impl<S: AsSocket> AsSocket for StreamWriter<S> {
    fn as_socket(&self) -> BorrowedSocket<'_> {
        self.stream.as_socket()
    }
}

#[cfg(windows)]
impl<S: AsRawSocket> AsRawSocket for StreamWriter<S> {
    fn as_raw_socket(&self) -> RawSocket {
        self.stream.as_raw_socket()
    }
}

fn split_cloned<S>(
    stream: S,
    cloned: io::Result<S>,
) -> Result<(StreamReader<S>, StreamWriter<S>), SplitIoError<S>> {
    match cloned {
        Ok(reader_stream) => {
            let id = next_split_id();
            Ok((
                StreamReader {
                    id,
                    stream: reader_stream,
                },
                StreamWriter { id, stream },
            ))
        }
        Err(error) => Err(SplitIoError {
            original: stream,
            error,
        }),
    }
}

fn split_shared<S: Copy>(stream: S) -> (StreamReader<S>, StreamWriter<S>) {
    let id = next_split_id();
    (StreamReader { id, stream }, StreamWriter { id, stream })
}

fn try_join_stream<S>(
    reader: StreamReader<S>,
    writer: StreamWriter<S>,
) -> Result<S, JoinIoError<StreamReader<S>, StreamWriter<S>>> {
    if reader.id != writer.id {
        return Err(JoinIoError { reader, writer });
    }
    drop(reader);
    Ok(writer.stream)
}

impl SplitIo for TcpStream {
    type Reader = StreamReader<Self>;
    type Writer = StreamWriter<Self>;

    fn split(self) -> Result<(Self::Reader, Self::Writer), SplitIoError<Self>> {
        let cloned = self.try_clone();
        split_cloned(self, cloned)
    }

    fn try_join(
        reader: Self::Reader,
        writer: Self::Writer,
    ) -> Result<Self, JoinIoError<Self::Reader, Self::Writer>> {
        try_join_stream(reader, writer)
    }
}

impl SplitIo for &TcpStream {
    type Reader = StreamReader<Self>;
    type Writer = StreamWriter<Self>;

    fn split(self) -> Result<(Self::Reader, Self::Writer), SplitIoError<Self>> {
        Ok(split_shared(self))
    }

    fn try_join(
        reader: Self::Reader,
        writer: Self::Writer,
    ) -> Result<Self, JoinIoError<Self::Reader, Self::Writer>> {
        try_join_stream(reader, writer)
    }
}

#[cfg(unix)]
impl SplitIo for UnixStream {
    type Reader = StreamReader<Self>;
    type Writer = StreamWriter<Self>;

    fn split(self) -> Result<(Self::Reader, Self::Writer), SplitIoError<Self>> {
        let cloned = self.try_clone();
        split_cloned(self, cloned)
    }

    fn try_join(
        reader: Self::Reader,
        writer: Self::Writer,
    ) -> Result<Self, JoinIoError<Self::Reader, Self::Writer>> {
        try_join_stream(reader, writer)
    }
}

#[cfg(unix)]
impl SplitIo for &UnixStream {
    type Reader = StreamReader<Self>;
    type Writer = StreamWriter<Self>;

    fn split(self) -> Result<(Self::Reader, Self::Writer), SplitIoError<Self>> {
        Ok(split_shared(self))
    }

    fn try_join(
        reader: Self::Reader,
        writer: Self::Writer,
    ) -> Result<Self, JoinIoError<Self::Reader, Self::Writer>> {
        try_join_stream(reader, writer)
    }
}

impl<R: io::Read, W: io::Write> SplitIo for (R, W) {
    type Reader = R;
    type Writer = W;

    fn split(self) -> Result<(Self::Reader, Self::Writer), SplitIoError<Self>> {
        Ok(self)
    }

    fn try_join(
        reader: Self::Reader,
        writer: Self::Writer,
    ) -> Result<Self, JoinIoError<Self::Reader, Self::Writer>> {
        Ok((reader, writer))
    }
}

/// Combines an independent reader `R` and writer `W` into a single
/// bidirectional handle implementing [`io::Read`], [`io::Write`], and
/// [`SplitIo`].
///
/// This is useful for pairing distinct standard library handles such as
/// `(`[`io::Stdin`]`, `[`io::Stdout`]`)`, `(`[`io::PipeReader`]`,
/// `[`io::PipeWriter`]`)`, or `(`[`std::process::ChildStdout`]`,
/// `[`std::process::ChildStdin`]`)` into a unified `Read + Write` stream.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Debug)]
pub struct IoPair<R, W> {
    /// Underlying reader half.
    pub reader: R,
    /// Underlying writer half.
    pub writer: W,
}

impl<R, W> IoPair<R, W> {
    /// Constructs a combined bidirectional I/O handle from `reader` and
    /// `writer`.
    pub const fn new(reader: R, writer: W) -> Self {
        Self { reader, writer }
    }

    /// Returns a shared reference to the reader half.
    pub const fn as_reader(&self) -> &R {
        &self.reader
    }

    /// Returns a shared reference to the writer half.
    pub const fn as_writer(&self) -> &W {
        &self.writer
    }

    /// Returns a mutable reference to the reader half.
    pub fn as_reader_mut(&mut self) -> &mut R {
        &mut self.reader
    }

    /// Returns a mutable reference to the writer half.
    pub fn as_writer_mut(&mut self) -> &mut W {
        &mut self.writer
    }

    /// Consumes the pair and returns the underlying `(reader, writer)` tuple.
    pub fn into_parts(self) -> (R, W) {
        (self.reader, self.writer)
    }
}

impl IoPair<io::Stdin, io::Stdout> {
    /// Constructs an [`IoPair`] over standard input and standard output.
    pub fn stdio() -> Self {
        Self {
            reader: io::stdin(),
            writer: io::stdout(),
        }
    }
}

impl<R, W> From<(R, W)> for IoPair<R, W> {
    fn from((reader, writer): (R, W)) -> Self {
        Self { reader, writer }
    }
}

impl<R, W> From<IoPair<R, W>> for (R, W) {
    fn from(pair: IoPair<R, W>) -> Self {
        (pair.reader, pair.writer)
    }
}

impl<R: io::Read, W> io::Read for IoPair<R, W> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.reader.read(buf)
    }

    fn read_vectored(&mut self, bufs: &mut [io::IoSliceMut<'_>]) -> io::Result<usize> {
        self.reader.read_vectored(bufs)
    }

    fn read_to_end(&mut self, buf: &mut Vec<u8>) -> io::Result<usize> {
        self.reader.read_to_end(buf)
    }

    fn read_to_string(&mut self, buf: &mut String) -> io::Result<usize> {
        self.reader.read_to_string(buf)
    }

    fn read_exact(&mut self, buf: &mut [u8]) -> io::Result<()> {
        self.reader.read_exact(buf)
    }
}

impl<R: io::BufRead, W> io::BufRead for IoPair<R, W> {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        self.reader.fill_buf()
    }

    fn consume(&mut self, amt: usize) {
        self.reader.consume(amt);
    }
}

impl<R, W: io::Write> io::Write for IoPair<R, W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.writer.write(buf)
    }

    fn write_vectored(&mut self, bufs: &[io::IoSlice<'_>]) -> io::Result<usize> {
        self.writer.write_vectored(bufs)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.writer.flush()
    }

    fn write_all(&mut self, buf: &[u8]) -> io::Result<()> {
        self.writer.write_all(buf)
    }

    fn write_fmt(&mut self, args: fmt::Arguments<'_>) -> io::Result<()> {
        self.writer.write_fmt(args)
    }
}

impl<R: io::Read, W: io::Write> SplitIo for IoPair<R, W> {
    type Reader = R;
    type Writer = W;

    fn split(self) -> Result<(Self::Reader, Self::Writer), SplitIoError<Self>> {
        Ok((self.reader, self.writer))
    }

    fn try_join(
        reader: Self::Reader,
        writer: Self::Writer,
    ) -> Result<Self, JoinIoError<Self::Reader, Self::Writer>> {
        Ok(Self { reader, writer })
    }
}

#[cfg(test)]
mod test {
    use std::io::{BufRead, Cursor, IoSlice, IoSliceMut, Read, Write};
    use std::net::TcpListener;
    use std::thread;

    use super::*;

    fn connected_tcp_pair() -> (TcpStream, TcpStream) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let client = TcpStream::connect(addr).unwrap();
        let (server, _) = listener.accept().unwrap();
        (client, server)
    }

    #[test]
    fn tcp_stream_split_concurrent_rw_and_rejoin() {
        let (client, mut server) = connected_tcp_pair();
        let expected_peer = client.peer_addr().unwrap();

        let (mut reader, mut writer) = client.split().unwrap();
        assert_eq!(reader.as_stream().peer_addr().unwrap(), expected_peer);
        assert_eq!(writer.as_ref().peer_addr().unwrap(), expected_peer);
        #[cfg(any(unix, target_os = "wasi"))]
        assert_ne!(reader.as_raw_fd(), writer.as_raw_fd());

        let server_handle = thread::spawn(move || {
            let mut incoming = [0u8; 4];
            server.read_exact(&mut incoming).unwrap();
            assert_eq!(&incoming, b"ping");
            server.write_all(b"pong").unwrap();
            server
        });

        writer.write_all(b"ping").unwrap();
        writer.flush().unwrap();
        let mut reply = [0u8; 4];
        reader.read_exact(&mut reply).unwrap();
        assert_eq!(&reply, b"pong");

        let mut server = server_handle.join().unwrap();
        let mut reunited = TcpStream::join(reader, writer);

        reunited.write_all(b"done").unwrap();
        let mut tail = [0u8; 4];
        server.read_exact(&mut tail).unwrap();
        assert_eq!(&tail, b"done");
    }

    #[test]
    fn tcp_stream_try_join_rejects_mismatched_halves_and_preserves_them() {
        let (c1, _s1) = connected_tcp_pair();
        let (c2, _s2) = connected_tcp_pair();

        let (r1, w1) = c1.split().unwrap();
        let (r2, w2) = c2.split().unwrap();

        let err = TcpStream::try_join(r1, w2).unwrap_err();
        assert!(err.to_string().contains("different streams"));

        let _rejoined1 = TcpStream::join(err.reader, w1);
        let _rejoined2 = TcpStream::join(r2, err.writer);
    }

    #[test]
    #[should_panic(expected = "not produced by the same split()")]
    fn tcp_stream_join_panics_on_mismatched_halves() {
        let (c1, _s1) = connected_tcp_pair();
        let (c2, _s2) = connected_tcp_pair();

        let (r1, _w1) = c1.split().unwrap();
        let (_r2, w2) = c2.split().unwrap();

        let _ = TcpStream::join(r1, w2);
    }

    #[test]
    fn borrowed_tcp_stream_splits_and_rejoins() {
        let (client, mut server) = connected_tcp_pair();
        let (mut reader, mut writer) = (&client).split().unwrap();

        writer.write_all(b"hi").unwrap();
        let mut buf = [0u8; 2];
        server.read_exact(&mut buf).unwrap();
        assert_eq!(&buf, b"hi");

        server.write_all(b"ok").unwrap();
        reader.read_exact(&mut buf).unwrap();
        assert_eq!(&buf, b"ok");

        let rejoined = <&TcpStream>::join(reader, writer);
        assert!(core::ptr::eq(rejoined, &client));
    }

    #[cfg(unix)]
    #[test]
    fn unix_stream_and_ref_split_and_rejoin() {
        let (a, mut b) = UnixStream::pair().unwrap();
        let (mut reader, mut writer) = a.split().unwrap();
        assert_ne!(reader.as_raw_fd(), writer.as_raw_fd());

        writer.write_all(b"unix").unwrap();
        let mut buf = [0u8; 4];
        b.read_exact(&mut buf).unwrap();
        assert_eq!(&buf, b"unix");

        b.write_all(b"back").unwrap();
        reader.read_exact(&mut buf).unwrap();
        assert_eq!(&buf, b"back");

        let reunited = UnixStream::join(reader, writer);
        let (mut ref_r, mut ref_w) = (&reunited).split().unwrap();
        ref_w.write_all(b"ref!").unwrap();
        b.read_exact(&mut buf).unwrap();
        assert_eq!(&buf, b"ref!");
        b.write_all(b"pong").unwrap();
        ref_r.read_exact(&mut buf).unwrap();
        assert_eq!(&buf, b"pong");

        let rejoined_ref = <&UnixStream>::join(ref_r, ref_w);
        assert!(core::ptr::eq(rejoined_ref, &reunited));
    }

    #[test]
    fn pipe_tuple_and_io_pair_split_and_rejoin() {
        let (pipe_reader, pipe_writer) = io::pipe().unwrap();
        let (mut r, mut w) = (pipe_reader, pipe_writer).split().unwrap();
        w.write_all(b"abc").unwrap();
        let mut buf = [0u8; 3];
        r.read_exact(&mut buf).unwrap();
        assert_eq!(&buf, b"abc");

        let mut pair = IoPair::from(<(io::PipeReader, io::PipeWriter)>::join(r, w));
        pair.write_all(b"xyz").unwrap();
        pair.read_exact(&mut buf).unwrap();
        assert_eq!(&buf, b"xyz");

        let (r, w) = pair.split().unwrap();
        let _pair = IoPair::join(r, w);
    }

    #[test]
    fn io_pair_bufread_accessors_and_error_conversions() {
        let mut pair = IoPair::new(Cursor::new(b"line1\nline2".to_vec()), Vec::<u8>::new());
        let mut line = String::new();
        pair.read_line(&mut line).unwrap();
        assert_eq!(line, "line1\n");
        pair.write_all(b"out").unwrap();
        assert_eq!(pair.as_writer(), b"out");
        assert_eq!(pair.as_reader().position(), 6);
        let (_, written) = pair.into_parts();
        assert_eq!(written, b"out");

        let _stdio = IoPair::stdio();

        let (ok_r, ok_w) = split_cloned(41u8, Ok(42u8)).unwrap();
        assert_eq!((*ok_r.as_stream(), *ok_w.as_stream()), (42u8, 41u8));
        let split_err = split_cloned(42u8, Err(io::Error::other("dup failed"))).unwrap_err();
        assert_eq!(split_err.original, 42u8);
        assert_eq!(split_err.to_string(), "dup failed");
        assert!(split_err.source().is_some());
        let io_err: io::Error = split_err.into();
        assert_eq!(io_err.kind(), io::ErrorKind::Other);

        let join_err = JoinIoError {
            reader: 1u8,
            writer: 2u8,
        };
        let io_err: io::Error = join_err.into();
        assert_eq!(io_err.kind(), io::ErrorKind::InvalidInput);
    }

    #[test]
    fn stream_halves_and_io_pair_forward_all_io_and_fd_methods() {
        let (client, mut server) = connected_tcp_pair();
        let (mut reader, mut writer) = client.split().unwrap();
        assert!(!format!("{reader:?}").is_empty());
        assert!(!format!("{writer:?}").is_empty());
        assert_eq!(
            reader.as_ref().local_addr().unwrap(),
            writer.as_stream().local_addr().unwrap()
        );
        #[cfg(any(unix, target_os = "wasi"))]
        {
            assert!(reader.as_fd().as_raw_fd() >= 0);
            assert!(writer.as_fd().as_raw_fd() >= 0);
        }

        let server_thread = thread::spawn(move || {
            let mut buf = [0u8; 13];
            server.read_exact(&mut buf).unwrap();
            assert_eq!(&buf, b"hello world 7");
            server.write_all(b"abcdUTF8!").unwrap();
            // Drop server to signal EOF for read_to_end / read_to_string.
        });

        assert_eq!(writer.write(b"he").unwrap(), 2);
        assert_eq!(
            writer
                .write_vectored(&[IoSlice::new(b"llo "), IoSlice::new(b"world ")])
                .unwrap(),
            10
        );
        write!(writer, "{}", 7).unwrap();
        writer.flush().unwrap();

        let mut one = [0u8; 1];
        assert_eq!(reader.read(&mut one).unwrap(), 1);
        assert_eq!(&one, b"a");
        let mut two = [0u8; 2];
        let mut rest_one = [0u8; 1];
        let n = reader
            .read_vectored(&mut [IoSliceMut::new(&mut two), IoSliceMut::new(&mut rest_one)])
            .unwrap();
        assert_eq!(n, 3);
        assert_eq!(&two, b"bc");
        assert_eq!(&rest_one, b"d");

        let mut text = String::new();
        reader.read_to_string(&mut text).unwrap();
        assert_eq!(text, "UTF8!");
        let mut empty_end = Vec::new();
        assert_eq!(reader.read_to_end(&mut empty_end).unwrap(), 0);

        server_thread.join().unwrap();

        let mut pair = IoPair::new(Cursor::new(b"12345678".to_vec()), Vec::<u8>::new());
        pair.as_reader_mut().set_position(1);
        pair.as_writer_mut().push(b'[');
        let mut p1 = [0u8; 1];
        assert_eq!(pair.read(&mut p1).unwrap(), 1);
        assert_eq!(&p1, b"2");
        let mut p2 = [0u8; 2];
        assert_eq!(
            pair.read_vectored(&mut [IoSliceMut::new(&mut p2)]).unwrap(),
            2
        );
        assert_eq!(&p2, b"34");
        let mut pstr = String::new();
        pair.read_to_string(&mut pstr).unwrap();
        assert_eq!(pstr, "5678");
        pair.as_reader_mut().set_position(6);
        let mut pend = Vec::new();
        pair.read_to_end(&mut pend).unwrap();
        assert_eq!(pend, b"78");

        assert_eq!(pair.write(b"a").unwrap(), 1);
        assert_eq!(
            pair.write_vectored(&[IoSlice::new(b"b"), IoSlice::new(b"c")])
                .unwrap(),
            2
        );
        write!(pair, "{}", 9).unwrap();
        pair.flush().unwrap();
        let (_, out): (_, Vec<u8>) = pair.into();
        assert_eq!(out, b"[abc9");
    }

    #[test]
    fn split_io_does_not_require_debug_on_halves() {
        struct NonDebugReader;
        struct NonDebugWriter;
        struct NonDebugStream(NonDebugReader, NonDebugWriter);

        impl io::Read for NonDebugReader {
            fn read(&mut self, _buf: &mut [u8]) -> io::Result<usize> {
                Ok(0)
            }
        }

        impl io::Write for NonDebugWriter {
            fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
                Ok(buf.len())
            }

            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }

        impl SplitIo for NonDebugStream {
            type Reader = NonDebugReader;
            type Writer = NonDebugWriter;

            fn split(self) -> Result<(Self::Reader, Self::Writer), SplitIoError<Self>> {
                Ok((self.0, self.1))
            }

            fn try_join(
                reader: Self::Reader,
                writer: Self::Writer,
            ) -> Result<Self, JoinIoError<Self::Reader, Self::Writer>> {
                Ok(Self(reader, writer))
            }
        }

        let stream = NonDebugStream(NonDebugReader, NonDebugWriter);
        let (mut r, mut w) = stream.split().ok().unwrap();
        assert_eq!(r.read(&mut [0u8; 2]).unwrap(), 0);
        assert_eq!(w.write(b"ok").unwrap(), 2);
        w.flush().unwrap();
        let _rejoined = NonDebugStream::join(r, w);
    }
}

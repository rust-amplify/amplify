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

//! Byte-confined streaming I/O adapters matching the compile-time bound
//! guarantees and conventions of the [`confined`] crate.
//!
//! # Overview
//!
//! - [`ConfinedReader<R, MIN, MAX>`]: bounds the number of bytes read from `R`
//!   to `MIN..=MAX`.
//! - [`ConfinedWriter<W, MIN, MAX>`]: bounds the number of bytes written to `W`
//!   to `MIN..=MAX`, with direct finalization into
//!   [`confined::Confined<Vec<u8>, MIN, MAX>`] via
//!   [`ConfinedWriter::into_confined`].
//! - [`ConfinedIo<S, READ_MIN, READ_MAX, WRITE_MIN, WRITE_MAX>`]: bounds both
//!   read and write byte counts independently on a bidirectional stream `S` and
//!   integrates with [`crate::io::SplitIo`] to split into [`ConfinedReader`]
//!   and [`ConfinedWriter`] halves and rejoin them without losing byte counts
//!   or confinement guarantees.
//! - Ready-made size aliases (`Tiny*`, `Small*`, `Medium*`, `Large*`,
//!   `NonEmpty*`) aligned with `confined`'s prefix conventions (`0..=U8MAX`,
//!   `0..=U16MAX`, `0..=U24MAX`, `0..=U32MAX`, and `1..=MAX`).

mod aliases;
mod error;
mod reader;
mod stream;
mod writer;

pub use aliases::{
    LargeIo, LargeReader, LargeWriter, MediumIo, MediumReader, MediumWriter, NonEmptyIo,
    NonEmptyReader, NonEmptyWriter, ONE, SmallIo, SmallReader, SmallWriter, TinyIo, TinyReader,
    TinyWriter, U8MAX, U16MAX, U24MAX, U32MAX, U40MAX, U48MAX, U56MAX, U64MAX, ZERO,
};
pub use error::{ConfinedIoError, UnfinishedIoError};
pub use reader::ConfinedReader;
pub use stream::ConfinedIo;
pub use writer::ConfinedWriter;

#[cfg(test)]
mod tests {
    #[cfg(any(unix, target_os = "wasi"))]
    use std::os::fd::{AsFd, AsRawFd};
    use std::collections::{HashSet, VecDeque};
    use std::error::Error as _;
    use std::io::{BufRead, Cursor, IoSlice, IoSliceMut, Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::thread;

    use amplify_num::u24;

    use super::*;
    use crate::io::{IoCounter, IoError, IoPair, JoinIoError, NullIo, SplitIo, SplitIoError};

    struct PartialStream {
        read_data: Vec<u8>,
        read_pos: usize,
        written: Vec<u8>,
        max_chunk: usize,
        fail_next_write: bool,
    }

    impl PartialStream {
        fn new(data: &[u8], max_chunk: usize) -> Self {
            Self {
                read_data: data.to_vec(),
                read_pos: 0,
                written: Vec::new(),
                max_chunk,
                fail_next_write: false,
            }
        }
    }

    impl Read for PartialStream {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            if buf.is_empty() || self.read_pos >= self.read_data.len() {
                return Ok(0);
            }
            let n = buf
                .len()
                .min(self.max_chunk)
                .min(self.read_data.len() - self.read_pos);
            buf[..n].copy_from_slice(&self.read_data[self.read_pos..self.read_pos + n]);
            self.read_pos += n;
            Ok(n)
        }
    }

    impl Write for PartialStream {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            if self.fail_next_write {
                self.fail_next_write = false;
                return Err(std::io::Error::from(std::io::ErrorKind::WouldBlock));
            }
            if buf.is_empty() {
                return Ok(0);
            }
            let n = buf.len().min(self.max_chunk);
            self.written.extend_from_slice(&buf[..n]);
            Ok(n)
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn confined_reader_enforces_max_without_overreading_inner_stream() {
        let mut cursor = Cursor::new(b"abcdefgh".as_slice());
        let mut reader = ConfinedReader::<_, 0, 5>::new(&mut cursor);
        let mut buf = [0u8; 8];

        assert_eq!(reader.read(&mut buf).unwrap(), 5);
        assert_eq!(&buf[..5], b"abcde");
        assert_eq!(reader.count(), 5);
        assert_eq!(reader.remaining(), 0);
        assert!(reader.is_exhausted());
        assert!(reader.is_satisfied());

        // Empty read at the limit succeeds without touching the inner cursor.
        assert_eq!(reader.read(&mut []).unwrap(), 0);

        // Non-empty read at the limit errors with UnexpectedEof +
        // ConfinedIoError::Oversize.
        let err = reader.read(&mut buf).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::UnexpectedEof);
        assert_eq!(
            ConfinedIoError::from_io_error(&err),
            Some(ConfinedIoError::Oversize { count: 6, max: 5 })
        );

        let _ = reader.release();
        // Underlying cursor was never read past byte 5!
        assert_eq!(cursor.position(), 5);
    }

    #[test]
    fn confined_reader_enforces_min_on_eof_and_release() {
        let mut short = ConfinedReader::<_, 4, 8>::from(Cursor::new(b"abc".as_slice()));
        let mut buf = [0u8; 8];
        assert_eq!(short.read(&mut buf).unwrap(), 3);
        assert!(!short.is_satisfied());
        assert_eq!(short.remaining_min(), 1);

        // EOF before MIN=4 reports Undersize.
        let err = short.read(&mut buf).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::UnexpectedEof);
        assert_eq!(
            ConfinedIoError::from_io_error(&err),
            Some(ConfinedIoError::Undersize { count: 3, min: 4 })
        );

        // Early `try_release` before MIN preserves the confined reader!
        let reader = ConfinedReader::<_, 2, 6>::new(Cursor::new(b"abcd".as_slice()));
        let unfinished = reader.try_release().unwrap_err();
        assert_eq!(
            unfinished.error,
            ConfinedIoError::Undersize { count: 0, min: 2 }
        );
        assert!(unfinished.source().is_some());
        let mut recovered = unfinished.into_stream();
        let mut two = [0u8; 2];
        recovered.read_exact(&mut two).unwrap();
        assert_eq!(&two, b"ab");
        let inner = recovered.try_release().unwrap();
        assert_eq!(inner.position(), 2);
    }

    #[test]
    fn confined_reader_read_exact_and_read_to_end_and_confined_bridges() {
        // read_exact rejects oversized request up front without reading
        // anything.
        let mut cursor = Cursor::new(b"abcdefgh".as_slice());
        let mut reader = ConfinedReader::<_, 0, 4>::new(&mut cursor);
        let mut big = [0u8; 5];
        let err = reader.read_exact(&mut big).unwrap_err();
        assert_eq!(
            ConfinedIoError::from_io_error(&err),
            Some(ConfinedIoError::Oversize { count: 5, max: 4 })
        );
        let _ = reader.release();
        assert_eq!(cursor.position(), 0);

        // read_to_end succeeds at exact MAX=5, fails on MAX+1=6, fails below
        // MIN=2, and works at 0..=0.
        let blob =
            ConfinedReader::<_, 2, 5>::read_confined(PartialStream::new(b"abcde", 2)).unwrap();
        assert_eq!(blob.as_slice(), b"abcde");
        assert_eq!(blob.len_u8(), 5);

        let oversize_err =
            ConfinedReader::<_, 2, 5>::read_confined(PartialStream::new(b"abcdef", 2)).unwrap_err();
        assert_eq!(
            ConfinedIoError::from_io_error(&oversize_err),
            Some(ConfinedIoError::Oversize { count: 6, max: 5 })
        );

        let undersize_err =
            ConfinedReader::<_, 2, 5>::read_confined(PartialStream::new(b"a", 2)).unwrap_err();
        assert_eq!(
            ConfinedIoError::from_io_error(&undersize_err),
            Some(ConfinedIoError::Undersize { count: 1, min: 2 })
        );

        let empty = ConfinedReader::<_, 0, 0>::read_confined(Cursor::new(b"".as_slice())).unwrap();
        assert!(empty.is_empty());
        assert!(ConfinedReader::<_, 0, 0>::read_confined(Cursor::new(b"x".as_slice())).is_err());

        // read_confined_string and read_to_string preserve existing prefix on
        // invalid UTF-8.
        let text =
            ConfinedReader::<_, 1, 16>::read_confined_string(Cursor::new("satoshi")).unwrap();
        assert_eq!(text.as_str(), "satoshi");

        let mut prefix = String::from("keep:");
        let mut bad_utf8 = TinyReader::new(Cursor::new([0xFF, 0xFE]));
        assert_eq!(
            bad_utf8.read_to_string(&mut prefix).unwrap_err().kind(),
            std::io::ErrorKind::InvalidData
        );
        assert_eq!(prefix, "keep:");
    }

    #[test]
    fn confined_reader_bufread_and_vectored_and_typed_counts() {
        let mut reader = SmallReader::new(Cursor::new(b"line1\nline2".as_slice()));
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        assert_eq!(line, "line1\n");
        assert_eq!(reader.count_u16(), 6);

        let widened: MediumReader<_> = reader.widen();
        assert_eq!(widened.count_u24(), u24::with(6));
        let widened_large: LargeReader<_> = widened.widen();
        assert_eq!(widened_large.count_u32(), 6);

        let mut vec_reader = TinyReader::new(Cursor::new(b"abcd".as_slice()));
        let mut a = [0u8; 2];
        let mut b = [0u8; 2];
        let n = vec_reader
            .read_vectored(&mut [IoSliceMut::new(&mut a), IoSliceMut::new(&mut b)])
            .unwrap();
        assert_eq!(n, 4);
        assert_eq!(vec_reader.count_u8(), 4);
    }

    #[test]
    fn confined_writer_inclusive_bounds_short_writes_errors_and_into_confined() {
        // Inclusive MAX: writing exactly MAX bytes succeeds!
        let mut writer = ConfinedWriter::<_, 2, 5>::vec();
        writer.write_all(b"abcde").unwrap();
        assert_eq!(writer.count_u8(), 5);
        assert!(writer.is_exhausted());
        assert_eq!(writer.write(&[]).unwrap(), 0);

        // One more byte fails without mutating the buffer.
        let err = writer.write(b"!").unwrap_err();
        assert_eq!(
            ConfinedIoError::from_io_error(&err),
            Some(ConfinedIoError::Oversize { count: 6, max: 5 })
        );
        let confined_blob = writer.into_confined().unwrap();
        assert_eq!(confined_blob.as_slice(), b"abcde");

        // Partial writes and transient errors do not corrupt `count`.
        let mut partial = ConfinedWriter::<_, 1, 5>::new(PartialStream::new(b"", 2));
        assert_eq!(partial.write(b"abcd").unwrap(), 2);
        assert_eq!(partial.count(), 2);
        partial.writer.fail_next_write = true;
        assert_eq!(
            partial.write(b"cd").unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
        assert_eq!(partial.count(), 2);

        // Oversized single write is rejected BEFORE touching the underlying
        // writer!
        let err = partial.write(b"cdef").unwrap_err();
        assert_eq!(
            ConfinedIoError::from_io_error(&err),
            Some(ConfinedIoError::Oversize { count: 6, max: 5 })
        );
        assert_eq!(partial.as_unconfined().written, b"ab");

        partial.write_all(b"cde").unwrap();
        assert_eq!(partial.count(), 5);
        assert_eq!(partial.release_checked().written, b"abcde");

        // NonEmptyWriter starting from empty Vec, failing before 1 byte,
        // succeeding after 1 byte.
        let empty_non_empty = NonEmptyWriter::<Vec<u8>, 4>::vec();
        let unfinished = empty_non_empty.into_confined().unwrap_err();
        assert_eq!(
            confined::Error::from(unfinished.error),
            confined::Error::Undersize { len: 0, min_len: 1 }
        );
        let mut retry = unfinished.into_stream();
        assert_eq!(
            retry
                .write_vectored(&[IoSlice::new(b"o"), IoSlice::new(b"k")])
                .unwrap(),
            2
        );
        let blob = retry.into_confined().unwrap();
        assert_eq!(blob.as_slice(), b"ok");

        // Pre-populated Vec exceeding MAX, undersize, and ok all on
        // ConfinedWriter<Vec<u8>, 1, 3>.
        let pre_under = ConfinedWriter::<_, 1, 3>::vec()
            .into_confined()
            .unwrap_err();
        let mut pre_ok = pre_under.into_stream();
        pre_ok.write_all(b"ab").unwrap();
        assert_eq!(pre_ok.into_confined().unwrap().as_slice(), b"ab");

        let mut pre = ConfinedWriter::<_, 1, 3>::new(vec![1, 2, 3, 4]);
        pre.write_all(&[5]).unwrap();
        // `pre.count == 1 <= 3`, while `pre.writer.len() == 5 > 3`!
        let pre_err = pre.into_confined().unwrap_err();
        assert_eq!(
            pre_err.error,
            ConfinedIoError::Oversize { count: 5, max: 3 }
        );
        partial_empty_and_flush_check();
    }

    fn partial_empty_and_flush_check() {
        let mut ps = PartialStream::new(b"", 2);
        assert_eq!(ps.write(b"").unwrap(), 0);
        ps.flush().unwrap();
    }

    #[test]
    fn confined_io_asymmetric_bounds_split_tcp_and_composition_with_counters() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let client = TcpStream::connect(addr).unwrap();
        let (mut server, _) = listener.accept().unwrap();

        // Read bounds: 4..=6 bytes; Write bounds: 2..=8 bytes, composed with
        // IoCounter!
        let counted_client = IoCounter::with(client);
        let mut stream = ConfinedIo::<_, 4, 6, 2, 8>::new(counted_client);

        let server_thread = thread::spawn(move || {
            let mut req = [0u8; 7];
            server.read_exact(&mut req).unwrap();
            assert_eq!(&req, b"hi-ping");
            server.write_all(b"pong!!").unwrap();
            server
        });

        stream.write_all(b"hi-").unwrap();
        assert_eq!(stream.write_count_u8(), 3);
        assert!(stream.is_write_satisfied());
        assert!(!stream.is_read_satisfied());

        // Split ConfinedIo<IoCounter<TcpStream>, 4, 6, 2, 8> across halves!
        let (mut reader, mut writer) = stream.split().unwrap();
        assert_eq!(writer.count(), 3);
        assert_eq!(writer.remaining(), 5);

        writer.write_all(b"ping").unwrap();
        writer.flush().unwrap();
        assert_eq!(writer.count(), 7);
        // Writing 2 more bytes (total 9 > 8) is rejected without sending
        // anything!
        assert!(writer.write(b"xx").is_err());

        let mut resp = [0u8; 6];
        reader.read_exact(&mut resp).unwrap();
        assert_eq!(&resp, b"pong!!");
        assert!(reader.is_exhausted());

        let reunited = ConfinedIo::<IoCounter<TcpStream>, 4, 6, 2, 8>::join(reader, writer);
        assert_eq!(reunited.counts(), (6, 7));
        assert!(reunited.is_satisfied());

        let wide: ConfinedIo<_, 0, 16, 0, 16> = reunited.widen();
        let inner_counter = wide.release();
        assert_eq!(inner_counter.counts(), (6, 7));

        let _server = server_thread.join().unwrap();

        // Also test IoPair conversions with ConfinedIo.
        let pair_io = ConfinedIo::<_, 0, 8, 0, 8>::from_pair(
            Cursor::new(b"abc".as_slice()),
            Vec::<u8>::new(),
        );
        let io_pair: IoPair<_, _> = pair_io.into();
        let (r_half, w_half) = io_pair.into_parts();
        let mut back = ConfinedIo::from_confined_halves(r_half, w_half);
        back.write_all(b"xy").unwrap();
        assert_eq!(back.write_count(), 2);
    }

    #[test]
    fn error_conversions_roundtrip_cleanly() {
        let over = ConfinedIoError::Oversize { count: 9, max: 8 };
        let io_err: std::io::Error = over.into();
        assert_eq!(io_err.kind(), std::io::ErrorKind::UnexpectedEof);
        assert_eq!(ConfinedIoError::from_io_error(&io_err), Some(over));

        let amp_io_err: IoError = over.into();
        assert_eq!(amp_io_err.kind(), std::io::ErrorKind::UnexpectedEof);

        let conf_err: confined::Error = over.into();
        assert_eq!(conf_err, confined::Error::Oversize { len: 9, max_len: 8 });

        let under = ConfinedIoError::Undersize { count: 1, min: 4 };
        assert!(under
            .to_string()
            .contains("less than lower confinement boundary"));
        assert!(over
            .to_string()
            .contains("exceeds upper confinement boundary"));
        assert_eq!(
            confined::Error::from(under),
            confined::Error::Undersize { len: 1, min_len: 4 }
        );

        let plain_eof = std::io::Error::from(std::io::ErrorKind::UnexpectedEof);
        assert_eq!(ConfinedIoError::from_io_error(&plain_eof), None);
        let other_payload = std::io::Error::other("custom");
        assert_eq!(ConfinedIoError::from_io_error(&other_payload), None);

        let unfinished = UnfinishedIoError {
            stream: 77u32,
            error: under,
        };
        assert_eq!(unfinished.to_string(), under.to_string());
        assert!(format!("{unfinished:?}").contains("UnfinishedIoError"));
        assert!(unfinished.source().is_some());
        assert_eq!(ConfinedIoError::from(unfinished), under);
        assert_eq!(
            confined::Error::from(unfinished),
            confined::Error::Undersize { len: 1, min_len: 4 }
        );
        let io_from_unf: std::io::Error = unfinished.into();
        assert_eq!(ConfinedIoError::from_io_error(&io_from_unf), Some(under));
        let amp_from_unf: IoError = unfinished.into();
        assert_eq!(amp_from_unf.kind(), std::io::ErrorKind::UnexpectedEof);
        assert_eq!(unfinished.into_parts(), (77u32, under));
    }

    #[derive(Clone, Debug, Default)]
    struct ScriptedIo {
        data: VecDeque<u8>,
        read_errors: VecDeque<std::io::ErrorKind>,
        write_errors: VecDeque<std::io::ErrorKind>,
        overreport_read: Option<usize>,
        overreport_write: Option<usize>,
        fail_split: bool,
        fail_join: bool,
        written: Vec<u8>,
    }

    impl ScriptedIo {
        fn with_data(bytes: &[u8]) -> Self {
            Self {
                data: bytes.iter().copied().collect(),
                ..Self::default()
            }
        }
    }

    impl Read for ScriptedIo {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            if let Some(kind) = self.read_errors.pop_front() {
                return Err(std::io::Error::from(kind));
            }
            if let Some(bad) = self.overreport_read.take() {
                return Ok(bad);
            }
            self.data.read(buf)
        }

        fn read_vectored(&mut self, bufs: &mut [IoSliceMut<'_>]) -> std::io::Result<usize> {
            if let Some(kind) = self.read_errors.pop_front() {
                return Err(std::io::Error::from(kind));
            }
            if let Some(bad) = self.overreport_read.take() {
                return Ok(bad);
            }
            let mut total = 0;
            for s in bufs {
                total += self.data.read(s).unwrap();
            }
            Ok(total)
        }
    }

    impl BufRead for ScriptedIo {
        fn fill_buf(&mut self) -> std::io::Result<&[u8]> {
            if let Some(kind) = self.read_errors.pop_front() {
                return Err(std::io::Error::from(kind));
            }
            self.data.fill_buf()
        }

        fn consume(&mut self, amt: usize) {
            self.data.consume(amt);
        }
    }

    impl Write for ScriptedIo {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            if let Some(kind) = self.write_errors.pop_front() {
                return Err(std::io::Error::from(kind));
            }
            if let Some(bad) = self.overreport_write.take() {
                return Ok(bad);
            }
            self.written.extend_from_slice(buf);
            Ok(buf.len())
        }

        fn write_vectored(&mut self, bufs: &[IoSlice<'_>]) -> std::io::Result<usize> {
            if let Some(kind) = self.write_errors.pop_front() {
                return Err(std::io::Error::from(kind));
            }
            if let Some(bad) = self.overreport_write.take() {
                return Ok(bad);
            }
            let mut total = 0;
            for s in bufs {
                self.written.extend_from_slice(s);
                total += s.len();
            }
            Ok(total)
        }

        fn flush(&mut self) -> std::io::Result<()> {
            if let Some(kind) = self.write_errors.pop_front() {
                return Err(std::io::Error::from(kind));
            }
            Ok(())
        }
    }

    impl SplitIo for ScriptedIo {
        type Reader = ScriptedIo;
        type Writer = ScriptedIo;

        fn split(self) -> Result<(Self::Reader, Self::Writer), SplitIoError<Self>> {
            if self.fail_split {
                return Err(SplitIoError {
                    original: self,
                    error: std::io::Error::from(std::io::ErrorKind::BrokenPipe),
                });
            }
            let fail_join = self.fail_join;
            Ok((
                ScriptedIo {
                    data: self.data,
                    fail_join,
                    ..ScriptedIo::default()
                },
                ScriptedIo {
                    written: self.written,
                    fail_join,
                    ..ScriptedIo::default()
                },
            ))
        }

        fn try_join(
            reader: Self::Reader,
            writer: Self::Writer,
        ) -> Result<Self, JoinIoError<Self::Reader, Self::Writer>> {
            if reader.fail_join || writer.fail_join {
                return Err(JoinIoError { reader, writer });
            }
            Ok(ScriptedIo {
                data: reader.data,
                written: writer.written,
                ..ScriptedIo::default()
            })
        }
    }

    #[test]
    fn confined_reader_exhaustively_covers_methods_bufread_vectored_and_retries() {
        let def_reader: ConfinedReader<ScriptedIo, 0, 4> = ConfinedReader::default();
        assert_eq!(def_reader.min_bound(), 0);
        assert_eq!(def_reader.max_bound(), 4);
        assert!(def_reader.as_unconfined().data.is_empty());
        assert!(def_reader.as_ref().data.is_empty());
        let cloned_inner = def_reader.to_unconfined();
        assert!(cloned_inner.data.is_empty());
        let _ = def_reader.into_unconfined();

        // read_exact: oversize, empty, Interrupted retry, short EOF after MIN
        // satisfied, and error on <ScriptedIo, 1, 4>.
        let mut s = ScriptedIo::with_data(b"ab");
        s.read_errors.push_back(std::io::ErrorKind::Interrupted);
        let mut r = ConfinedReader::<_, 1, 4>::new(s);
        assert_eq!(
            ConfinedIoError::from_io_error(&r.read_exact(&mut [0u8; 5]).unwrap_err()),
            Some(ConfinedIoError::Oversize { count: 5, max: 4 })
        );
        r.read_exact(&mut []).unwrap();
        let mut three = [0u8; 3];
        let eof_err = r.read_exact(&mut three).unwrap_err();
        assert_eq!(eof_err.kind(), std::io::ErrorKind::UnexpectedEof);
        assert_eq!(ConfinedIoError::from_io_error(&eof_err), None);
        assert_eq!(r.count(), 2);
        r.reader.read_errors.push_back(std::io::ErrorKind::TimedOut);
        assert_eq!(
            r.read_exact(&mut [0u8; 1]).unwrap_err().kind(),
            std::io::ErrorKind::TimedOut
        );
        r.reader.data.extend(b"z");
        let mut one_exact = [0u8; 1];
        r.read_exact(&mut one_exact).unwrap();
        assert_eq!(&one_exact, b"z");
        let _ = r.release_checked();

        // read & read_vectored: all branches on <ScriptedIo, 2, 4>.
        let s_vec = ScriptedIo::with_data(b"");
        let mut rv = ConfinedReader::<_, 2, 4>::new(s_vec);
        assert_eq!(rv.read(&mut []).unwrap(), 0);
        rv.reader.read_errors.push_back(std::io::ErrorKind::Other);
        assert_eq!(
            rv.read(&mut [0u8; 2]).unwrap_err().kind(),
            std::io::ErrorKind::Other
        );
        rv.reader.overreport_read = Some(10);
        assert_eq!(
            ConfinedIoError::from_io_error(&rv.read(&mut [0u8; 2]).unwrap_err()),
            Some(ConfinedIoError::Oversize { count: 10, max: 4 })
        );
        assert_eq!(
            ConfinedIoError::from_io_error(&rv.read(&mut [0u8; 2]).unwrap_err()),
            Some(ConfinedIoError::Undersize { count: 0, min: 2 })
        );
        rv.reader.data.extend(b"abcde");
        rv.reader.overreport_read = Some(10);
        let mut b1 = [0u8; 2];
        assert_eq!(
            ConfinedIoError::from_io_error(
                &rv.read_vectored(&mut [IoSliceMut::new(&mut b1)])
                    .unwrap_err()
            ),
            Some(ConfinedIoError::Oversize { count: 10, max: 4 })
        );
        rv.reader.read_errors.push_back(std::io::ErrorKind::Other);
        assert_eq!(
            rv.read_vectored(&mut [IoSliceMut::new(&mut b1)])
                .unwrap_err()
                .kind(),
            std::io::ErrorKind::Other
        );
        assert_eq!(rv.read_vectored(&mut []).unwrap(), 0);
        // Fallback when total (2 + 3 = 5) > remaining (4): reads first
        // non-empty slice (2 bytes).
        let mut b2 = [0u8; 3];
        assert_eq!(
            rv.read_vectored(&mut [
                IoSliceMut::new(&mut []),
                IoSliceMut::new(&mut b1),
                IoSliceMut::new(&mut b2),
            ])
            .unwrap(),
            2
        );
        assert_eq!(&b1, b"ab");
        // Exhaust remaining 2 bytes, then read and read_vectored at remaining
        // == 0 fail with Oversize.
        assert_eq!(
            rv.read_vectored(&mut [IoSliceMut::new(&mut b1)]).unwrap(),
            2
        );
        assert_eq!(
            rv.read(&mut [0u8; 1]).unwrap_err().kind(),
            std::io::ErrorKind::UnexpectedEof
        );
        assert_eq!(
            ConfinedIoError::from_io_error(
                &rv.read_vectored(&mut [IoSliceMut::new(&mut b1)])
                    .unwrap_err()
            ),
            Some(ConfinedIoError::Oversize { count: 5, max: 4 })
        );

        let mut rv_under = ConfinedReader::<_, 2, 4>::new(ScriptedIo::with_data(b""));
        assert_eq!(
            ConfinedIoError::from_io_error(
                &rv_under
                    .read_vectored(&mut [IoSliceMut::new(&mut b1)])
                    .unwrap_err()
            ),
            Some(ConfinedIoError::Undersize { count: 0, min: 2 })
        );

        // read_to_end, read_to_string, and read_confined_string: all branches
        // on <ScriptedIo, 1, 3>.
        let mut s_end = ScriptedIo::with_data(b"");
        s_end
            .read_errors
            .push_back(std::io::ErrorKind::PermissionDenied);
        assert_eq!(
            ConfinedReader::<_, 1, 3>::read_confined_string(s_end)
                .unwrap_err()
                .kind(),
            std::io::ErrorKind::PermissionDenied
        );
        let ok_conf_str =
            ConfinedReader::<_, 1, 3>::read_confined_string(ScriptedIo::with_data(b"hi")).unwrap();
        assert_eq!(ok_conf_str.as_str(), "hi");

        let mut r_all = ConfinedReader::<_, 1, 3>::new(ScriptedIo::with_data(b""));
        // 1) undersize on read_to_end
        assert_eq!(
            ConfinedIoError::from_io_error(&r_all.read_to_end(&mut Vec::new()).unwrap_err()),
            Some(ConfinedIoError::Undersize { count: 0, min: 1 })
        );
        // 2) overreport on read_to_end
        r_all.reader.overreport_read = Some(9);
        assert_eq!(
            ConfinedIoError::from_io_error(&r_all.read_to_end(&mut Vec::new()).unwrap_err()),
            Some(ConfinedIoError::Oversize { count: 9, max: 3 })
        );
        // 3) invalid UTF-8 on read_to_string (count becomes 2)
        r_all.reader.data.extend([0xFF, 0xFE]);
        assert_eq!(
            r_all.read_to_string(&mut String::new()).unwrap_err().kind(),
            std::io::ErrorKind::InvalidData
        );
        // 4) probe error and oversize at MAX=3, then ok at MAX=3, and append to
        //    non-empty String
        let mut r_str = ConfinedReader::<_, 1, 3>::new(ScriptedIo::with_data(b"ok!"));
        r_str
            .reader
            .read_errors
            .push_back(std::io::ErrorKind::Interrupted);
        r_str
            .reader
            .read_errors
            .push_back(std::io::ErrorKind::Interrupted);
        let mut prefix = String::from("msg:");
        assert_eq!(r_str.read_to_string(&mut prefix).unwrap(), 3);
        assert_eq!(prefix, "msg:ok!");
        // At count == 3: probe error, then probe > 0 (Oversize).
        r_str
            .reader
            .read_errors
            .push_back(std::io::ErrorKind::BrokenPipe);
        assert_eq!(
            r_str.read_to_end(&mut Vec::new()).unwrap_err().kind(),
            std::io::ErrorKind::BrokenPipe
        );
        r_str.reader.data.extend(b"!");
        assert_eq!(
            ConfinedIoError::from_io_error(&r_str.read_to_end(&mut Vec::new()).unwrap_err()),
            Some(ConfinedIoError::Oversize { count: 4, max: 3 })
        );

        // BufRead: error, undersize on empty, ok on empty when satisfied,
        // oversize when exhausted.
        let mut sb = ScriptedIo::with_data(b"");
        sb.read_errors.push_back(std::io::ErrorKind::Other);
        let mut rb = ConfinedReader::<_, 1, 2>::new(sb);
        assert_eq!(rb.fill_buf().unwrap_err().kind(), std::io::ErrorKind::Other);
        assert_eq!(
            ConfinedIoError::from_io_error(&rb.fill_buf().unwrap_err()),
            Some(ConfinedIoError::Undersize { count: 0, min: 1 })
        );

        let mut rb2 = ConfinedReader::<_, 1, 2>::new(ScriptedIo::with_data(b"xyz"));
        assert_eq!(rb2.fill_buf().unwrap(), b"xy");
        rb2.consume(2);
        assert_eq!(
            ConfinedIoError::from_io_error(&rb2.fill_buf().unwrap_err()),
            Some(ConfinedIoError::Oversize { count: 3, max: 2 })
        );

        let mut rb3 = ConfinedReader::<_, 0, 2>::new(ScriptedIo::with_data(b""));
        assert_eq!(rb3.fill_buf().unwrap(), b"");
    }

    #[test]
    fn confined_writer_and_stream_exhaustively_cover_methods_errors_and_fds() {
        let mut sink = ConfinedWriter::<NullIo, 1, 4>::sink();
        assert_eq!(sink.min_bound(), 1);
        assert_eq!(sink.max_bound(), 4);
        assert_eq!(sink.remaining_min(), 1);
        assert!(!sink.is_satisfied());
        assert_eq!(*sink.as_ref(), NullIo);
        assert_eq!(sink.to_unconfined(), NullIo);
        let unf = sink.try_release().unwrap_err();
        sink = unf.into_stream();
        sink.write_all(b"ab").unwrap();
        assert!(sink.is_satisfied());
        assert_eq!(sink.remaining_min(), 0);
        let _ = sink.try_release().unwrap();
        let mut sink = ConfinedWriter::<NullIo, 1, 4>::sink();
        sink.write_all(b"ab").unwrap();
        let small_w: SmallWriter<NullIo> = sink.widen();
        assert_eq!(small_w.count_u16(), 2);
        let wide_w: MediumWriter<NullIo> = small_w.widen();
        assert_eq!(wide_w.count_u24(), u24::with(2));
        let large_w: LargeWriter<NullIo> = wide_w.widen();
        assert_eq!(large_w.count_u32(), 2);
        assert_eq!(large_w.release(), NullIo);

        let def_w: ConfinedWriter<Vec<u8>, 0, 8> = ConfinedWriter::default();
        assert!(def_w.into_unconfined().is_empty());
        let cap_w = ConfinedWriter::<Vec<u8>, 0, 8>::with_capacity(64);
        assert!(cap_w.as_unconfined().capacity() >= 8);
        let try_ok = ConfinedWriter::<Vec<u8>, 0, U64MAX>::try_with_capacity(4).unwrap();
        assert!(try_ok.as_unconfined().capacity() >= 4);
        let try_err =
            ConfinedWriter::<Vec<u8>, 0, U64MAX>::try_with_capacity(usize::MAX).unwrap_err();
        assert_eq!(try_err, confined::Error::CapacityExceeded { len: 0 });

        // Writer all branches on ConfinedWriter<ScriptedIo, 0, 8>.
        let sw = ScriptedIo {
            overreport_write: Some(5),
            ..ScriptedIo::default()
        };
        let mut cw = ConfinedWriter::<_, 0, 8>::from(sw);
        assert_eq!(cw.write(b"").unwrap(), 0);
        assert_eq!(
            ConfinedIoError::from_io_error(&cw.write(b"123456789").unwrap_err()),
            Some(ConfinedIoError::Oversize { count: 9, max: 8 })
        );
        assert_eq!(
            cw.write(b"ab").unwrap_err().kind(),
            std::io::ErrorKind::InvalidData
        );
        cw.writer.overreport_write = Some(5);
        assert_eq!(
            cw.write_vectored(&[IoSlice::new(b"ab")])
                .unwrap_err()
                .kind(),
            std::io::ErrorKind::InvalidData
        );
        cw.writer.write_errors.push_back(std::io::ErrorKind::Other);
        assert_eq!(
            cw.write(b"ab").unwrap_err().kind(),
            std::io::ErrorKind::Other
        );
        cw.writer.write_errors.push_back(std::io::ErrorKind::Other);
        assert_eq!(
            cw.write_vectored(&[IoSlice::new(b"ab")])
                .unwrap_err()
                .kind(),
            std::io::ErrorKind::Other
        );
        cw.writer
            .write_errors
            .push_back(std::io::ErrorKind::BrokenPipe);
        assert_eq!(
            cw.flush().unwrap_err().kind(),
            std::io::ErrorKind::BrokenPipe
        );
        assert_eq!(cw.write_vectored(&[]).unwrap(), 0);
        assert_eq!(
            ConfinedIoError::from_io_error(
                &cw.write_vectored(&[IoSlice::new(b"12345"), IoSlice::new(b"6789")])
                    .unwrap_err()
            ),
            Some(ConfinedIoError::Oversize { count: 9, max: 8 })
        );
        assert_eq!(cw.write(b"ab").unwrap(), 2);
        assert_eq!(cw.write_vectored(&[IoSlice::new(b"cd")]).unwrap(), 2);
        cw.flush().unwrap();
        let _ = cw.try_release().unwrap();

        // ConfinedIo methods, typed counts, BufRead, Read/Write forwarders,
        // SplitIo errors, FDs.
        let null_io = ConfinedIo::<NullIo, 0, 4, 0, 4>::null();
        assert_eq!(null_io.to_unconfined(), NullIo);
        assert_eq!(*null_io.as_unconfined(), NullIo);
        assert_eq!(*null_io.as_ref(), NullIo);
        let def_io: ConfinedIo<NullIo, 0, 4, 0, 4> = ConfinedIo::default();
        assert_eq!(def_io.into_unconfined(), NullIo);

        let mut sio = ConfinedIo::<_, 2, 16, 2, 16>::new(ScriptedIo::with_data(b"hi\nthere"));
        assert_eq!(sio.read_min_bound(), 2);
        assert_eq!(sio.read_max_bound(), 16);
        assert_eq!(sio.write_min_bound(), 2);
        assert_eq!(sio.write_max_bound(), 16);
        assert_eq!(sio.read_remaining(), 16);
        assert_eq!(sio.read_remaining_min(), 2);
        assert_eq!(sio.write_remaining(), 16);
        assert_eq!(sio.write_remaining_min(), 2);
        assert!(!sio.is_read_exhausted());
        assert!(!sio.is_write_exhausted());
        assert!(sio.check().is_err());

        // Read fails release first, then write fails release once read is
        // satisfied.
        let unf1 = sio.try_release().unwrap_err();
        sio = unf1.into_stream();
        let mut line = String::new();
        sio.read_line(&mut line).unwrap();
        assert_eq!(line, "hi\n");
        assert!(sio.check_read().is_ok());
        assert!(sio.check().is_err());
        let unf2 = sio.try_release().unwrap_err();
        sio = unf2.into_stream();

        let mut one = [0u8; 1];
        assert_eq!(sio.read(&mut one).unwrap(), 1);
        let mut v1 = [0u8; 2];
        assert_eq!(
            sio.read_vectored(&mut [IoSliceMut::new(&mut v1)]).unwrap(),
            2
        );
        let mut rest_str = String::new();
        assert_eq!(sio.read_to_string(&mut rest_str).unwrap(), 2);
        assert_eq!(rest_str, "re");
        assert_eq!(sio.read_to_end(&mut Vec::new()).unwrap(), 0);

        assert_eq!(
            sio.write_vectored(&[IoSlice::new(b"x"), IoSlice::new(b"yz")])
                .unwrap(),
            3
        );
        sio.flush().unwrap();
        assert!(sio.check().is_ok());
        let _ = sio.clone().try_release().unwrap();

        let wide16: SmallIo<_> = sio.widen();
        assert_eq!(wide16.read_count_u16(), 8);
        assert_eq!(wide16.write_count_u16(), 3);
        let wide24: MediumIo<_> = wide16.widen();
        assert_eq!(wide24.read_count_u24(), u24::with(8));
        assert_eq!(wide24.write_count_u24(), u24::with(3));
        let wide32: LargeIo<_> = wide24.widen();
        assert_eq!(wide32.read_count_u32(), 8);
        assert_eq!(wide32.write_count_u32(), 3);
        let _ = wide32.release_checked();

        // SplitIo split & try_join error preservation on ConfinedIo.
        let mut fail_s = ScriptedIo::with_data(b"abcd");
        fail_s.fail_split = true;
        let mut fail_cio = ConfinedIo::<_, 0, 8, 0, 8>::new(fail_s);
        fail_cio.write_all(b"12").unwrap();
        let split_err = SplitIo::split(fail_cio).unwrap_err();
        assert_eq!(split_err.original.counts(), (0, 2));
        let mut orig = split_err.original.into_unconfined();
        orig.fail_split = false;
        orig.fail_join = true;
        let (mut rh, wh) = SplitIo::split(ConfinedIo::<_, 0, 8, 0, 8>::new(orig)).unwrap();
        rh.reader.fail_join = false;
        let join_err = ConfinedIo::<ScriptedIo, 0, 8, 0, 8>::try_join(rh, wh).unwrap_err();
        assert_eq!(join_err.reader.count(), 0);
        assert_eq!(join_err.writer.count(), 0);
        let (rh2, mut wh2) = (join_err.reader, join_err.writer);
        wh2.writer.fail_join = false;
        let ok_joined = ConfinedIo::<ScriptedIo, 0, 8, 0, 8>::join(rh2, wh2);
        let _ = ConfinedReader::<_, 0, 8>::new(ok_joined.into_unconfined());

        // Exercise derived Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug
        // on confined types.
        let cr1 = ConfinedReader::<Vec<u8>, 0, 8>::new(vec![10]);
        let cr2 = cr1.clone();
        let cr3 = ConfinedReader::<Vec<u8>, 0, 8>::from_raw_parts(vec![10], 1);
        let cr4 = ConfinedReader::<Vec<u8>, 0, 8>::new(vec![20]);
        assert_eq!(cr1, cr2);
        assert_ne!(cr1, cr3);
        assert_ne!(cr1, cr4);
        assert!(cr1 < cr3);
        assert!(cr1 < cr4);
        assert_eq!(cr1.cmp(&cr2), core::cmp::Ordering::Equal);
        assert!(!format!("{cr1:?}").is_empty());
        let mut cr_set = HashSet::new();
        assert!(cr_set.insert(cr1));

        let cw1 = ConfinedWriter::<Vec<u8>, 0, 8>::new(vec![10]);
        let cw2 = cw1.clone();
        let cw3 = ConfinedWriter::<Vec<u8>, 0, 8>::from_raw_parts(vec![10], 1);
        let cw4 = ConfinedWriter::<Vec<u8>, 0, 8>::new(vec![20]);
        assert_eq!(cw1, cw2);
        assert_ne!(cw1, cw3);
        assert_ne!(cw1, cw4);
        assert!(cw1 < cw3);
        assert!(cw1 < cw4);
        assert_eq!(cw1.cmp(&cw2), core::cmp::Ordering::Equal);
        assert!(!format!("{cw1:?}").is_empty());
        let mut cw_set = HashSet::new();
        assert!(cw_set.insert(cw1));

        let cs1 = ConfinedIo::<Vec<u8>, 0, 8, 0, 8>::new(vec![10]);
        let cs2 = cs1.clone();
        let cs3 = ConfinedIo::<Vec<u8>, 0, 8, 0, 8>::from_raw_parts(vec![10], 1, 0);
        let cs4 = ConfinedIo::<Vec<u8>, 0, 8, 0, 8>::from_raw_parts(vec![10], 0, 1);
        let cs5 = ConfinedIo::<Vec<u8>, 0, 8, 0, 8>::new(vec![20]);
        assert_eq!(cs1, cs2);
        assert_ne!(cs1, cs3);
        assert_ne!(cs1, cs4);
        assert_ne!(cs1, cs5);
        assert!(cs1 < cs3);
        assert!(cs1 < cs4);
        assert!(cs1 < cs5);
        assert_eq!(cs1.cmp(&cs2), core::cmp::Ordering::Equal);
        assert!(!format!("{cs1:?}").is_empty());
        let mut cs_set = HashSet::new();
        assert!(cs_set.insert(cs1));

        let ce1 = ConfinedIoError::Undersize { count: 0, min: 1 };
        let ce2 = ce1;
        let ce3 = ConfinedIoError::Oversize { count: 9, max: 8 };
        assert_eq!(ce1, ce2);
        assert!(ce1 < ce3);
        assert_eq!(ce1.cmp(&ce2), core::cmp::Ordering::Equal);
        let mut ce_set = HashSet::new();
        assert!(ce_set.insert(ce1));

        let uf1 = UnfinishedIoError {
            stream: vec![1u8],
            error: ce1,
        };
        let uf2 = uf1.clone();
        let uf3 = UnfinishedIoError {
            stream: vec![2u8],
            error: ce1,
        };
        let uf4 = UnfinishedIoError {
            stream: vec![1u8],
            error: ce3,
        };
        assert_eq!(uf1, uf2);
        assert_ne!(uf1, uf3);
        assert_ne!(uf1, uf4);
        let mut uf_set = HashSet::new();
        assert!(uf_set.insert(uf1));

        #[cfg(any(unix, target_os = "wasi"))]
        {
            let (pr, pw) = std::io::pipe().unwrap();
            let cr_fd = TinyReader::new(pr);
            let cw_fd = TinyWriter::new(pw);
            assert_eq!(cr_fd.as_raw_fd(), cr_fd.as_fd().as_raw_fd());
            assert_eq!(cw_fd.as_raw_fd(), cw_fd.as_fd().as_raw_fd());

            let (u1, _u2) = std::os::unix::net::UnixStream::pair().unwrap();
            let cio_fd = TinyIo::new(u1);
            assert_eq!(cio_fd.as_raw_fd(), cio_fd.as_fd().as_raw_fd());
        }
    }

    #[test]
    #[should_panic(expected = "confined reader count exceeds upper confinement bound")]
    fn confined_reader_from_raw_parts_panics_above_max() {
        let _ = ConfinedReader::<NullIo, 0, 2>::from_raw_parts(NullIo, 3);
    }

    #[test]
    #[should_panic(expected = "confined writer count exceeds upper confinement bound")]
    fn confined_writer_from_raw_parts_panics_above_max() {
        let _ = ConfinedWriter::<NullIo, 0, 2>::from_raw_parts(NullIo, 3);
    }

    #[test]
    #[should_panic(expected = "confined stream read count exceeds upper read bound")]
    fn confined_io_from_raw_parts_panics_above_read_max() {
        let _ = ConfinedIo::<NullIo, 0, 2, 0, 4>::from_raw_parts(NullIo, 3, 0);
    }

    #[test]
    #[should_panic(expected = "confined stream write count exceeds upper write bound")]
    fn confined_io_from_raw_parts_panics_above_write_max() {
        let _ = ConfinedIo::<NullIo, 0, 4, 0, 2>::from_raw_parts(NullIo, 0, 3);
    }

    #[test]
    #[should_panic(expected = "stream confinement lower bound is not satisfied")]
    fn confined_reader_release_checked_panics_when_undersize() {
        let r = ConfinedReader::<NullIo, 1, 4>::new(NullIo);
        let _ = r.release_checked();
    }

    #[test]
    #[should_panic(expected = "stream confinement lower bound is not satisfied")]
    fn confined_writer_release_checked_panics_when_undersize() {
        let w = ConfinedWriter::<NullIo, 1, 4>::sink();
        let _ = w.release_checked();
    }

    #[test]
    #[should_panic(expected = "stream confinement lower bounds are not satisfied")]
    fn confined_io_release_checked_panics_when_undersize() {
        let io = ConfinedIo::<NullIo, 1, 4, 1, 4>::null();
        let _ = io.release_checked();
    }
}

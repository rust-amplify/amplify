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
    use std::error::Error as _;
    use std::io::{BufRead, Cursor, IoSlice, IoSliceMut, Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::thread;

    use amplify_num::u24;

    use super::*;
    use crate::io::{IoCounter, IoError, IoPair, SplitIo};

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

        // Pre-populated Vec exceeding MAX is caught by into_confined.
        let mut pre = ConfinedWriter::<_, 1, 3>::new(vec![1, 2, 3, 4]);
        pre.write_all(&[5]).unwrap_or_else(|_| unreachable!());
        // Wait: `pre.count == 1 <= 3`, while `pre.writer.len() == 5 > 3`!
        let pre_err = pre.into_confined().unwrap_err();
        assert_eq!(
            pre_err.error,
            ConfinedIoError::Oversize { count: 5, max: 3 }
        );
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
    }
}

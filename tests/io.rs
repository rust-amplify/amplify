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

#[cfg(feature = "alloc")]
#[test]
fn deque_traits_work_across_vec_deque_variants() {
    use amplify::alloc::borrow::Cow;
    use amplify::alloc::boxed::Box;
    use amplify::alloc::collections::VecDeque;
    use amplify::io::{AsDequeMut, AsDequeRef};

    let mut owned = VecDeque::from([10u8, 20, 30]);
    assert_eq!(owned.as_deque_ref().front(), Some(&10));
    owned.as_deque_mut().push_front(5);

    let shared = &owned;
    assert_eq!(shared.as_deque_ref().len(), 4);

    let mut owned2 = owned.clone();
    let mut_ref = &mut owned2;
    assert_eq!(mut_ref.as_deque_ref().len(), 4);
    mut_ref.as_deque_mut().push_back(40);
    assert_eq!(owned2.len(), 5);

    let mut boxed = Box::new(owned2);
    assert_eq!(boxed.as_deque_ref().len(), 5);
    boxed.as_deque_mut().push_back(50);
    assert_eq!(boxed.as_deque_ref().len(), 6);

    let mut cow: Cow<'_, VecDeque<u8>> = Cow::Borrowed(&owned);
    assert_eq!(cow.as_deque_ref().len(), 4);
    cow.as_deque_mut().push_back(99);
    assert_eq!(cow.as_deque_ref().len(), 5);
}

#[cfg(feature = "std")]
mod std_tests {
    use std::collections::{HashSet, VecDeque};
    use std::error::Error as _;
    use std::io::{self, BufRead, Cursor, IoSlice, IoSliceMut, Read, Seek, SeekFrom, Write};
    use std::net::{TcpListener, TcpStream};
    #[cfg(any(unix, target_os = "wasi"))]
    use std::os::fd::{AsFd, AsRawFd};
    #[cfg(unix)]
    use std::os::unix::net::UnixStream;
    use std::thread;

    use amplify::confined::{
        self, ONE, U8MAX, U16MAX, U24MAX, U32MAX, U40MAX, U48MAX, U56MAX, U64MAX, ZERO,
    };
    use amplify::io::confined as io_confined;
    use amplify::io::{
        ConfinedIo, ConfinedIoError, ConfinedReader, ConfinedWriter, CursorDeque, IoCounter,
        IoError, IoPair, JoinIoError, LargeIo, LargeReader, LargeWriter, MediumIo, MediumReader,
        MediumWriter, NonEmptyIo, NonEmptyReader, NonEmptyWriter, NullIo, ReadCounter, SmallIo,
        SmallReader, SmallWriter, SplitIo, SplitIoError, TinyIo, TinyReader, TinyWriter,
        UnfinishedIoError, WriteCounter,
    };
    use amplify::num::u24;

    fn connected_tcp_pair() -> (TcpStream, TcpStream) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let client = TcpStream::connect(addr).unwrap();
        let (server, _) = listener.accept().unwrap();
        (client, server)
    }

    #[test]
    fn io_error_public_api_conversions_ordering_and_hashing() {
        let e_kind = IoError::from(io::ErrorKind::AddrInUse);
        assert_eq!(e_kind.kind(), io::ErrorKind::AddrInUse);
        assert!(e_kind.source().is_none());

        let e_custom_a = IoError::from(io::Error::new(io::ErrorKind::InvalidInput, "bad-a"));
        let e_custom_a2 = e_custom_a.clone();
        let e_custom_b = IoError::from(io::Error::new(io::ErrorKind::InvalidInput, "bad-b"));

        assert_eq!(e_custom_a, e_custom_a2);
        assert_ne!(e_custom_a, e_custom_b);
        assert_ne!(e_custom_a, e_kind);
        assert!(e_custom_a < e_custom_b);
        assert_eq!(
            e_custom_a.partial_cmp(&e_custom_a2),
            Some(core::cmp::Ordering::Equal)
        );
        assert_ne!(e_kind.cmp(&e_custom_a), core::cmp::Ordering::Equal);

        let mut set = HashSet::new();
        assert!(set.insert(e_custom_a2.clone()));
        assert!(!set.insert(e_custom_a2));

        assert!(format!("{e_custom_a}").contains("bad-a"));
        assert!(format!("{e_custom_a:?}").contains("bad-a"));

        let back_with_details: io::Error = e_custom_a.into();
        assert_eq!(back_with_details.to_string(), "bad-a");

        let back_from_kind: io::Error = e_kind.into();
        assert_eq!(back_from_kind.kind(), io::ErrorKind::AddrInUse);
        assert!(back_from_kind.get_ref().is_none());
    }

    #[test]
    fn counters_and_null_io_integration_with_pipes_tcp_and_cursor_deque() {
        let mut null = NullIo;
        assert_eq!(null.read(&mut [0u8; 4]).unwrap(), 0);
        assert_eq!(
            null.read_vectored(&mut [IoSliceMut::new(&mut [0u8; 2])])
                .unwrap(),
            0
        );
        assert_eq!(null.fill_buf().unwrap(), b"");
        null.consume(0);
        assert_eq!(null.write(b"abc").unwrap(), 3);
        assert_eq!(
            null.write_vectored(&[IoSlice::new(b"x"), IoSlice::new(b"yz")])
                .unwrap(),
            3
        );
        null.flush().unwrap();
        assert_eq!(null.seek(SeekFrom::Start(4)).unwrap(), 0);
        assert!(null.seek(SeekFrom::Current(-2)).is_err());
        let (nr, nw) = SplitIo::split(null).unwrap();
        assert_eq!(NullIo::join(nr, nw), NullIo);

        // Standalone ReadCounter / WriteCounter / IoCounter constructors.
        let mut rep = ReadCounter::repeat(0xAB);
        let mut four = [0u8; 4];
        rep.read_exact(&mut four).unwrap();
        assert_eq!(four, [0xAB; 4]);
        assert_eq!(rep.count(), 4);
        assert_eq!(rep.reset(), 4);
        assert!(rep.is_empty());

        let mut z = ReadCounter::zeros();
        z.read_exact(&mut four).unwrap();
        assert_eq!(four, [0; 4]);
        let (z_count, _) = z.into_parts();
        assert_eq!(z_count, 4);

        let mut sink = WriteCounter::new();
        sink.write_all(b"sizing").unwrap();
        assert_eq!(sink.count(), 6);
        assert_eq!(sink.reset(), 6);

        let mut deque_cursor: CursorDeque<VecDeque<u8>> = CursorDeque::default();
        assert!(deque_cursor.is_empty());
        deque_cursor.get_mut().extend(*b"line-a\nline-b");
        assert!(!deque_cursor.is_empty());
        assert!(!format!("{deque_cursor:?}").is_empty());
        assert_eq!(deque_cursor.clone(), deque_cursor);
        deque_cursor.seek(SeekFrom::Start(0)).unwrap();
        assert_eq!(
            deque_cursor.seek(SeekFrom::Current(-1)).unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
        let mut counted_deque = ReadCounter::with(deque_cursor);
        let mut first_line = String::new();
        counted_deque.read_line(&mut first_line).unwrap();
        assert_eq!(first_line, "line-a\n");
        assert_eq!(counted_deque.count(), 7);
        let mut deque_cursor = counted_deque.into_reader();
        deque_cursor.seek(SeekFrom::End(0)).unwrap();
        assert_eq!(deque_cursor.fill_buf().unwrap(), b"");
        deque_cursor.write_all(b"!").unwrap();
        deque_cursor.flush().unwrap();
        deque_cursor.set_position(u64::MAX);
        assert_eq!(
            deque_cursor.write(b"x").unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
        assert_eq!(deque_cursor.into_inner().len(), 14);

        // Fragmented VecDeque read + fill_buf across s1, s2, and EOF + write
        // resize & overflow.
        let mut frag = VecDeque::with_capacity(8);
        frag.extend([1u8, 2, 3, 4, 5, 6]);
        for _ in 0..4 {
            frag.pop_front();
        }
        frag.extend([7u8, 8, 9, 10, 11]);
        let s1_len = frag.as_slices().0.len();
        let mut frag_cur = CursorDeque::new(frag);
        assert!(!frag_cur.fill_buf().unwrap().is_empty());
        frag_cur.set_position(s1_len as u64);
        assert!(!frag_cur.fill_buf().unwrap().is_empty());
        let mut s2_byte = [0u8; 1];
        assert_eq!(frag_cur.read(&mut s2_byte).unwrap(), 1);
        frag_cur.set_position(0);
        let mut all_frag = [0u8; 7];
        assert_eq!(frag_cur.read(&mut all_frag).unwrap(), 7);
        assert_eq!(frag_cur.read(&mut s2_byte).unwrap(), 0);
        assert_eq!(frag_cur.fill_buf().unwrap(), b"");
        frag_cur.set_position(0);
        frag_cur.write_all(&[9, 9]).unwrap();
        frag_cur.set_position(10);
        frag_cur.write_all(&[1]).unwrap();
        frag_cur.set_position(u64::MAX);
        assert!(frag_cur.write(b"x").is_err());
        let rc_cell = std::cell::RefCell::new(VecDeque::from([1u8, 2]));
        assert_eq!(
            amplify::io::AsDequeRef::as_deque_ref(&rc_cell.borrow_mut()).len(),
            2
        );
        let rw_cell = std::sync::RwLock::new(VecDeque::from([1u8, 2, 3]));
        assert_eq!(
            amplify::io::AsDequeRef::as_deque_ref(&rw_cell.write().unwrap()).len(),
            3
        );

        // IoCounter over TcpStream and &TcpStream +UnixStream.
        let (client, mut server) = connected_tcp_pair();
        let server_worker = thread::spawn(move || {
            let mut incoming = [0u8; 6];
            server.read_exact(&mut incoming).unwrap();
            assert_eq!(&incoming, b"abcdef");
            server.write_all(b"12345").unwrap();
            server
        });

        let mut tcp_counter = IoCounter::with(client);
        let (cr, mut cw) = tcp_counter.split().unwrap();
        cw.write_all(b"abc").unwrap();
        tcp_counter = IoCounter::<TcpStream>::join(cr, cw);
        {
            let (mut br, mut bw) = tcp_counter.as_stream().split().unwrap();
            bw.write_all(b"def").unwrap();
            let mut head = [0u8; 2];
            br.read_exact(&mut head).unwrap();
            assert_eq!(&head, b"12");
            let _ = <&TcpStream>::join(br, bw);
        }
        let mut rest = [0u8; 3];
        tcp_counter.read_exact(&mut rest).unwrap();
        assert_eq!(&rest, b"345");
        assert_eq!(tcp_counter.counts(), (3, 3));
        let (rc_val, wc_val, _raw_tcp) = tcp_counter.into_parts();
        assert_eq!((rc_val, wc_val), (3, 3));
        let _ = server_worker.join().unwrap();

        #[cfg(unix)]
        {
            let (u_client, mut u_server) = UnixStream::pair().unwrap();
            let (mut ur, mut uw) = u_client.split().unwrap();
            uw.write_all(b"ux").unwrap();
            let mut two = [0u8; 2];
            u_server.read_exact(&mut two).unwrap();
            assert_eq!(&two, b"ux");
            u_server.write_all(b"ok").unwrap();
            ur.read_exact(&mut two).unwrap();
            assert_eq!(&two, b"ok");
            let u_rejoined = UnixStream::join(ur, uw);
            let (mut ur_ref, mut uw_ref) = (&u_rejoined).split().unwrap();
            uw_ref.write_all(b"!!").unwrap();
            u_server.read_exact(&mut two).unwrap();
            assert_eq!(&two, b"!!");
            u_server.write_all(b"??").unwrap();
            ur_ref.read_exact(&mut two).unwrap();
            assert_eq!(&two, b"??");
            let _ = <&UnixStream>::join(ur_ref, uw_ref);
        }
    }

    #[test]
    fn confined_aliases_constants_errors_and_collection_bridges() {
        assert_eq!(io_confined::ZERO, ZERO);
        assert_eq!(io_confined::ONE, ONE);
        assert_eq!(io_confined::U8MAX, U8MAX);
        assert_eq!(io_confined::U16MAX, U16MAX);
        assert_eq!(io_confined::U24MAX, U24MAX);
        assert_eq!(io_confined::U32MAX, U32MAX);
        assert_eq!(io_confined::U40MAX, U40MAX);
        assert_eq!(io_confined::U48MAX, U48MAX);
        assert_eq!(io_confined::U56MAX, U56MAX);
        assert_eq!(io_confined::U64MAX, U64MAX);

        // Reader aliases & typed counts + widen.
        let mut tr = TinyReader::new(Cursor::new(b"tiny".as_slice()));
        let mut four = [0u8; 4];
        tr.read_exact(&mut four).unwrap();
        assert_eq!(tr.count_u8(), 4);
        let sr: SmallReader<_> = tr.widen();
        assert_eq!(sr.count_u16(), 4);
        let mr: MediumReader<_> = sr.widen();
        assert_eq!(mr.count_u24(), u24::with(4));
        let lr: LargeReader<_> = mr.widen();
        assert_eq!(lr.count_u32(), 4);
        let _ = lr.release();

        // NonEmptyReader + read_confined + read_confined_string.
        let non_empty_bytes = NonEmptyReader::<_, 8>::read_confined(Cursor::new(b"rgb")).unwrap();
        assert_eq!(non_empty_bytes.as_slice(), b"rgb");
        let non_empty_str =
            NonEmptyReader::<_, 8>::read_confined_string(Cursor::new("lnp")).unwrap();
        assert_eq!(non_empty_str.as_str(), "lnp");

        // Writer aliases & into_confined + widen + try_with_capacity.
        let mut tw: ConfinedWriter<Vec<u8>, ZERO, U8MAX> = TinyWriter::with_capacity(16);
        let _: ConfinedReader<NullIo, ZERO, U8MAX> = ConfinedReader::new(NullIo);
        tw.write_all(b"frame").unwrap();
        assert_eq!(tw.count_u8(), 5);
        let sw: SmallWriter<Vec<u8>> = tw.widen();
        assert_eq!(sw.count_u16(), 5);
        let mw: MediumWriter<Vec<u8>> = sw.widen();
        assert_eq!(mw.count_u24(), u24::with(5));
        let lw: LargeWriter<Vec<u8>> = mw.widen();
        assert_eq!(lw.count_u32(), 5);
        let vec_confined = lw.into_confined().unwrap();
        assert_eq!(vec_confined.as_slice(), b"frame");

        let cap_ok = NonEmptyWriter::<Vec<u8>, 16>::try_with_capacity(8).unwrap();
        let unf = cap_ok.into_confined().unwrap_err();
        assert_eq!(unf.to_string(), unf.error.to_string());
        assert!(format!("{unf:?}").contains("UnfinishedIoError"));
        assert!(unf.source().is_some());
        let (_, raw_err) = unf.into_parts();
        assert_eq!(
            confined::Error::from(raw_err),
            confined::Error::Undersize { len: 0, min_len: 1 }
        );
        let over_err = ConfinedIoError::Oversize { count: 10, max: 8 };
        assert_eq!(
            confined::Error::from(over_err),
            confined::Error::Oversize {
                len: 10,
                max_len: 8
            }
        );
        let io_err: io::Error = over_err.into();
        assert_eq!(ConfinedIoError::from_io_error(&io_err), Some(over_err));
        let amp_err: IoError = over_err.into();
        assert_eq!(amp_err.kind(), io::ErrorKind::UnexpectedEof);
    }

    #[test]
    fn confined_io_end_to_end_framed_protocol_with_split_and_counters() {
        let (pipe_a_r, pipe_a_w) = io::pipe().unwrap();
        let (pipe_b_r, pipe_b_w) = io::pipe().unwrap();

        let client_raw = IoCounter::from_pair(pipe_b_r, pipe_a_w);
        let mut server_raw = IoPair::new(pipe_a_r, pipe_b_w);

        #[cfg(any(unix, target_os = "wasi"))]
        {
            assert!(client_raw.as_stream().as_reader().as_fd().as_raw_fd() >= 0);
            assert!(client_raw.as_stream().as_writer().as_raw_fd() >= 0);
        }

        let server_job = thread::spawn(move || {
            let mut req = [0u8; 5];
            server_raw.read_exact(&mut req).unwrap();
            assert_eq!(&req, b"HELLO");
            server_raw.write_all(b"WORLD!").unwrap();
            drop(server_raw);
        });

        let client_confined: NonEmptyIo<_, 8> = ConfinedIo::new(client_raw);
        let (mut r_half, mut w_half) = client_confined.split().unwrap();

        w_half.write_all(b"HELLO").unwrap();
        w_half.flush().unwrap();

        let mut reply = String::new();
        r_half.read_to_string(&mut reply).unwrap();
        assert_eq!(reply, "WORLD!");

        let reunited = NonEmptyIo::<IoCounter<IoPair<_, _>>, 8>::join(r_half, w_half);
        assert!(reunited.is_satisfied());
        assert_eq!(reunited.counts(), (6, 5));

        let tiny: TinyIo<_> = reunited.widen();
        assert_eq!(tiny.read_count_u8(), 6);
        assert_eq!(tiny.write_count_u8(), 5);
        let small: SmallIo<_> = tiny.widen();
        assert_eq!(small.read_count_u16(), 6);
        assert_eq!(small.write_count_u16(), 5);
        let medium: MediumIo<_> = small.widen();
        assert_eq!(medium.read_count_u24(), u24::with(6));
        assert_eq!(medium.write_count_u24(), u24::with(5));
        let large: LargeIo<_> = medium.widen();
        assert_eq!(large.read_count_u32(), 6);
        assert_eq!(large.write_count_u32(), 5);

        let counted = large.release();
        assert_eq!(counted.counts(), (6, 5));

        server_job.join().unwrap();

        let _ = IoPair::stdio();
        let def_pair: IoPair<Vec<u8>, Vec<u8>> = IoPair::default();
        let cloned_pair = def_pair.clone();
        assert_eq!(def_pair, cloned_pair);
        assert_eq!(def_pair.cmp(&cloned_pair), core::cmp::Ordering::Equal);
        assert!(!format!("{def_pair:?}").is_empty());
        let mut pair_set = HashSet::new();
        assert!(pair_set.insert(def_pair));

        let s_err: SplitIoError<u8> = SplitIoError {
            original: 1,
            error: io::Error::from(io::ErrorKind::Other),
        };
        assert!(!format!("{s_err:?}").is_empty());
        let j_err = JoinIoError {
            reader: vec![1u8],
            writer: vec![2u8],
        };
        let j_err2 = j_err.clone();
        assert_eq!(j_err, j_err2);
        assert!(!format!("{j_err:?}").is_empty());
        let mut j_set = HashSet::new();
        assert!(j_set.insert(j_err.clone()));
        assert!(j_err.to_string().contains("different streams"));
        let _: io::Error = j_err.into();
        let unf_sample = UnfinishedIoError {
            stream: NullIo,
            error: ConfinedIoError::Undersize { count: 0, min: 1 },
        };
        let _: io::Error = unf_sample.into();
        let _: IoError = unf_sample.into();
        let _: confined::Error = unf_sample.into();
    }
}

#[test]
fn traits_module_exports_shared_traits_and_root_reexports() {
    use core::fmt;
    use amplify::array::{Array, Bytes4, FromSliceError};
    use amplify::error::MultiError;
    use amplify::strategy::Holder;
    use amplify::traits::{ByteArray, IntoMultiError, Wrapper, WrapperMut};

    #[derive(Copy, Clone, PartialEq, Eq)]
    struct Tag(Bytes4);

    impl fmt::Debug for Tag {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.debug_tuple("Tag").field(&self.0.to_byte_array()).finish()
        }
    }

    impl Wrapper for Tag {
        type Inner = Bytes4;

        fn from_inner(inner: Self::Inner) -> Self {
            Self(inner)
        }

        fn as_inner(&self) -> &Self::Inner {
            &self.0
        }

        fn into_inner(self) -> Self::Inner {
            self.0
        }
    }

    impl WrapperMut for Tag {
        fn as_inner_mut(&mut self) -> &mut Self::Inner {
            &mut self.0
        }
    }

    let mut tag = Tag::from_byte_array([1, 2, 3, 4]);
    tag.as_inner_mut()[0] = 9;
    assert_eq!(tag.to_byte_array(), [9, 2, 3, 4]);
    assert_eq!(
        Tag::from_slice([1, 2]),
        Err(FromSliceError {
            expected: 4,
            actual: 2,
        })
    );

    let arr: Array<u8, 4> = tag.into_inner();
    let held: Holder<_, ()> = Holder::new(arr);
    assert_eq!(held.as_type().to_byte_array(), [9, 2, 3, 4]);

    let err = FromSliceError {
        expected: 4,
        actual: 2,
    };
    let multi = err.into_multi_error();
    assert_eq!(multi, MultiError::A(err));

    #[allow(deprecated)]
    {
        use amplify::traits::{AsAny, Dumb};
        let _: fn(&u8) -> &dyn core::any::Any = <u8 as AsAny>::as_any;
        let _: u8 = <u8 as Dumb>::dumb();
    }
}

#[cfg(feature = "alloc")]
#[test]
fn io_module_retains_deque_traits_and_std_io_types() {
    use amplify::alloc::collections::VecDeque;
    use amplify::io::{AsDequeMut, AsDequeRef};

    let mut deque = VecDeque::from([1u8, 2, 3]);
    assert_eq!(deque.as_deque_ref().len(), 3);
    deque.as_deque_mut().push_back(4);
    assert_eq!(deque.as_deque_ref().len(), 4);

    #[cfg(feature = "std")]
    {
        use std::io::{Read, Write};
        use amplify::io::{
            ConfinedIo, ConfinedIoError, ConfinedReader, ConfinedWriter, CursorDeque, IoCounter,
            IoError, IoPair, JoinIoError, NullIo, ReadCounter, SmallReader, SplitIo, SplitIoError,
            StreamReader, StreamWriter, TinyIo, TinyWriter, UnfinishedIoError, WriteCounter,
        };

        let mut counter = WriteCounter::default();
        counter.write_all(b"abc").unwrap();
        assert_eq!(counter.count, 3);
        assert_eq!(counter.count(), 3);

        let mut read_counter = ReadCounter::with(std::io::Cursor::new(b"abcd"));
        let mut rbuf = [0u8; 2];
        read_counter.read_exact(&mut rbuf).unwrap();
        assert_eq!(&rbuf, b"ab");
        assert_eq!(read_counter.count(), 2);

        let cursor = CursorDeque::new(deque);
        let mut confined = ConfinedIo::<_, 0, 8>::from(cursor);
        assert_eq!(confined.read_count(), 0);
        assert_eq!(confined.write_count(), 0);
        let mut four = [0u8; 4];
        confined.read_exact(&mut four).unwrap();
        assert_eq!(four, [1, 2, 3, 4]);
        assert_eq!(confined.read_count_u8(), 4);
        let _cursor = confined.release();

        let io_err = IoError::from(std::io::ErrorKind::UnexpectedEof);
        assert_eq!(io_err.kind(), std::io::ErrorKind::UnexpectedEof);

        let (pipe_r, pipe_w) = std::io::pipe().unwrap();
        let counted_pair = IoCounter::from_pair(pipe_r, pipe_w);
        let confined_pair: TinyIo<_> = ConfinedIo::from(counted_pair);
        let (mut r, mut w): (ConfinedReader<_, 0, 255>, ConfinedWriter<_, 0, 255>) =
            confined_pair.split().unwrap();
        w.write_all(b"io").unwrap();
        let mut out = [0u8; 2];
        r.read_exact(&mut out).unwrap();
        assert_eq!(&out, b"io");
        let rejoined: TinyIo<IoCounter<IoPair<_, _>>> = SplitIo::join(r, w);
        assert_eq!(rejoined.counts(), (2, 2));
        let inner_counted = rejoined.release();
        assert_eq!(inner_counted.counts(), (2, 2));

        let mut tiny_buf = TinyWriter::vec();
        tiny_buf.write_all(b"ok").unwrap();
        assert_eq!(tiny_buf.into_confined().unwrap().as_slice(), b"ok");

        let mut small_r = SmallReader::new(NullIo);
        assert_eq!(small_r.read(&mut [0u8; 1]).unwrap(), 0);
        let under: UnfinishedIoError<_> = ConfinedReader::<_, 1, 4>::new(NullIo)
            .try_release()
            .unwrap_err();
        let conf_err: ConfinedIoError = under.into();
        assert_eq!(conf_err, ConfinedIoError::Undersize { count: 0, min: 1 });

        let (u_a, u_b) = std::net::TcpListener::bind("127.0.0.1:0")
            .and_then(|l| {
                let c = std::net::TcpStream::connect(l.local_addr()?)?;
                let (s, _) = l.accept()?;
                Ok((c, s))
            })
            .unwrap();
        let (r1, w1): (StreamReader<_>, StreamWriter<_>) = u_a.split().unwrap();
        let (r2, w2) = u_b.split().unwrap();
        let join_err: JoinIoError<_, _> = std::net::TcpStream::try_join(r1, w2).unwrap_err();
        let _ = std::net::TcpStream::join(join_err.reader, w1);
        let _ = std::net::TcpStream::join(r2, join_err.writer);
        let split_err = SplitIoError {
            original: NullIo,
            error: std::io::Error::from(std::io::ErrorKind::Other),
        };
        assert_eq!(split_err.original, NullIo);
    }
}

#[cfg(feature = "std")]
#[test]
fn flags_module_exposes_concrete_flag_types() {
    use amplify::flags::{AllSet, FilteredIter, FlagNo, FlagRef, FlagVec, ParseError};

    let mut flags = FlagVec::new();
    let idx: FlagNo = 2;
    flags.set(idx);
    let _: Option<FlagRef<'_>> = None;
    assert_eq!(AllSet::new(&flags).count(), 1);
    assert_eq!(FilteredIter::new(&flags, flags.clone()).count(), 1);
    assert_eq!("invalid".parse::<FlagVec>(), Err(ParseError));
}

#[cfg(all(feature = "c_raw", not(target_arch = "wasm32")))]
#[test]
fn ffi_traits_are_exported_under_traits_and_ffi() {
    use amplify::ffi::{TryAsStr, TryFromRawStr, TryIntoRawStr, TryIntoString};
    use amplify::traits::{
        TryAsStr as TraitTryAsStr, TryFromRawStr as TraitTryFromRawStr,
        TryIntoRawStr as TraitTryIntoRawStr, TryIntoString as TraitTryIntoString,
    };

    fn assert_same_traits<
        S: TryIntoRawStr + TraitTryIntoRawStr + TryFromRawStr + TraitTryFromRawStr,
    >() {
    }
    fn assert_ptr_traits<C: TryAsStr + TraitTryAsStr, M: TryIntoString + TraitTryIntoString>() {}

    assert_same_traits::<String>();
    assert_ptr_traits::<*const core::ffi::c_char, *mut core::ffi::c_char>();
}

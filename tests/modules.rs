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
            ConfinedIo, CursorDeque, IoError, IoPair, JoinIoError, SplitIo, SplitIoError,
            StreamReader, StreamWriter, WriteCounter,
        };

        let mut counter = WriteCounter::default();
        counter.write_all(b"abc").unwrap();
        assert_eq!(counter.count, 3);

        let cursor = CursorDeque::new(deque);
        let confined = ConfinedIo::<_, 8>::from(cursor);
        assert_eq!(confined.pos(), 0);

        let io_err = IoError::from(std::io::ErrorKind::UnexpectedEof);
        assert_eq!(io_err.kind(), std::io::ErrorKind::UnexpectedEof);

        let (pipe_r, pipe_w) = std::io::pipe().unwrap();
        let (mut r, mut w) = IoPair::new(pipe_r, pipe_w).split().unwrap();
        w.write_all(b"io").unwrap();
        let mut out = [0u8; 2];
        r.read_exact(&mut out).unwrap();
        assert_eq!(&out, b"io");
        let _pair = IoPair::join(r, w);

        let _: Option<SplitIoError<std::net::TcpStream>> = None;
        let _: Option<
            JoinIoError<StreamReader<std::net::TcpStream>, StreamWriter<std::net::TcpStream>>,
        > = None;
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

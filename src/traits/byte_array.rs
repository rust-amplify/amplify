// Rust language amplification library providing multiple generic trait
// implementations, type wrappers, derive macros and other language enhancements
//
// Written in 2020-2026 by
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

use crate::array::{Array, FromSliceError};
use crate::Wrapper;

/// Trait which does a blanket implementation for all types wrapping [`Array`]s
pub trait ByteArray<const LEN: usize>: Sized {
    /// Constructs a wrapper type around a byte array.
    fn from_byte_array(val: impl Into<[u8; LEN]>) -> Self;

    /// Constructs a byte array from the slice. Errors if the slice length
    /// doesn't match `LEN` constant generic.
    fn from_slice(slice: impl AsRef<[u8]>) -> Result<Self, FromSliceError>;

    /// Constructs a byte array from a slice, panicking if the slice length
    /// doesn't match the `LEN` constant generic.
    fn from_slice_checked(slice: impl AsRef<[u8]>) -> Self {
        Self::from_slice(slice).expect("slice length not matching type requirements")
    }

    /// Returns a byte array representation stored in the wrapped type.
    fn to_byte_array(&self) -> [u8; LEN];
}

impl<Id, const LEN: usize, const REVERSE_STR: bool> ByteArray<LEN> for Id
where
    Id: Wrapper<Inner = Array<u8, LEN, REVERSE_STR>>,
{
    fn from_byte_array(val: impl Into<[u8; LEN]>) -> Self {
        Self::from_inner(Array::from_inner(val.into()))
    }

    fn from_slice(slice: impl AsRef<[u8]>) -> Result<Self, FromSliceError> {
        Array::try_from(slice.as_ref()).map(Self::from_inner)
    }

    fn to_byte_array(&self) -> [u8; LEN] {
        self.as_inner().into_inner()
    }
}

#[cfg(test)]
mod test {
    use core::fmt;

    use super::*;
    use crate::Bytes4;

    #[derive(Copy, Clone, PartialEq, Eq)]
    struct TestId(Bytes4);

    impl fmt::Debug for TestId {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.debug_tuple("TestId")
                .field(&self.0.to_byte_array())
                .finish()
        }
    }

    impl Wrapper for TestId {
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

    #[test]
    fn byte_array_blanket_impl() {
        let id = TestId::from_byte_array([1, 2, 3, 4]);
        assert_eq!(id.to_byte_array(), [1, 2, 3, 4]);
        assert_eq!(TestId::from_slice([1, 2, 3, 4]), Ok(id));
        assert_eq!(TestId::from_slice_checked([1, 2, 3, 4]), id);
        assert_eq!(
            TestId::from_slice([1, 2, 3]),
            Err(FromSliceError {
                expected: 4,
                actual: 3,
            })
        );
    }

    #[test]
    #[should_panic(expected = "slice length not matching type requirements")]
    fn byte_array_from_slice_checked_panics_on_mismatch() {
        let _ = TestId::from_slice_checked([1, 2]);
    }
}

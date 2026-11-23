// Rust language amplification library providing multiple generic trait
// implementations, type wrappers, derive macros and other language enhancements
//
// Written in 2019-2026 by
//     Dr. Maxim Orlovsky <orlovsky@ubideco.org>
//     Martin Habovstiak <martin.habovstiak@gmail.com>
//
// To the extent possible under law, the author(s) have dedicated all
// copyright and related and neighboring rights to this software to
// the public domain worldwide. This software is distributed without
// any warranty.
//
// You should have received a copy of the MIT License
// along with this software.
// If not, see <https://opensource.org/licenses/MIT>.

use core::convert::Infallible;
use core::error::Error;

use crate::error::MultiError;

pub trait IntoMultiError: Error + Sized {
    fn into_multi_error(self) -> MultiError<Self, Infallible, Infallible>;
}

impl<E: Error> IntoMultiError for E {
    fn into_multi_error(self) -> MultiError<Self, Infallible, Infallible> {
        MultiError::A(self)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::array::FromSliceError;

    #[test]
    fn into_multi_error_wraps_in_variant_a() {
        let err = FromSliceError {
            expected: 4,
            actual: 2,
        };
        assert_eq!(err.into_multi_error(), MultiError::A(err));
    }
}

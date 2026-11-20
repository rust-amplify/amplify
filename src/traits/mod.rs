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

//! Shared traits provided by the crate.

mod as_any;
mod byte_array;
mod dumb;
mod error;
#[cfg(all(feature = "c_raw", not(target_arch = "wasm32")))]
#[allow(unsafe_code)]
pub mod ffi;
pub mod wrapper;

#[allow(deprecated)]
pub use as_any::AsAny;
pub use byte_array::ByteArray;
#[allow(deprecated)]
pub use dumb::Dumb;
pub use error::IntoMultiError;
#[cfg(all(feature = "c_raw", not(target_arch = "wasm32")))]
pub use ffi::{TryAsStr, TryFromRawStr, TryIntoRawStr, TryIntoString};
pub use wrapper::{Wrapper, WrapperMut};

// Rust language amplification library providing multiple generic trait
// implementations, type wrappers, derive macros and other language enhancements
//
// Written in 2019-2022 by
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

//! The umbrella crate for the Rust Amplify family.
//!
//! This crate re-exports [`confined`] and [`amplify_num`] and provides a small
//! set of shared wrappers, arrays, and utilities.

#![cfg_attr(not(feature = "std"), no_std)]
#![deny(unsafe_code)]

#[cfg(any(feature = "alloc", test))]
pub extern crate alloc;
#[cfg(test)]
extern crate std;
pub extern crate collections;

pub use amplify_num as num;
#[cfg(feature = "hex")]
pub use amplify_num::hex;
pub use confined;
#[cfg(feature = "ascii")]
pub use confined::ascii;

#[cfg(feature = "derive")]
pub use amplify_derive::{Display, Error, From, Getters, Wrapper, WrapperMut};
#[cfg(feature = "derive")]
#[deprecated(note = "trait upcasting is stable; use `core::any::Any` directly")]
pub use amplify_derive::AsAny;

#[cfg(feature = "proc_attr")]
pub use amplify_syn as proc_attr;
#[cfg(feature = "proc_attr")]
pub use proc_attr::ident;

#[cfg(feature = "stringly_conversions")]
pub use stringly_conversions;

#[cfg(feature = "alloc")]
pub use collections::{bmap, bset, deque, heap, list, s};
#[cfg(feature = "std")]
pub use collections::{map, set};
#[cfg(feature = "alloc")]
pub use confined::{
    medium_blob, medium_deque, medium_heap, medium_list, medium_s, medium_vec, small_blob,
    small_deque, small_heap, small_list, small_s, small_vec, tiny_blob, tiny_deque, tiny_heap,
    tiny_list, tiny_s, tiny_vec, medium_bmap, medium_bset, small_bmap, small_bset, tiny_bmap,
    tiny_bset,
};
#[cfg(feature = "std")]
pub use confined::{medium_map, medium_set, small_map, small_set, tiny_map, tiny_set};
#[cfg(all(feature = "indexmap", feature = "std"))]
pub use confined::{medium_imap, medium_iset, small_imap, small_iset, tiny_imap, tiny_iset};

mod macros;
#[doc(hidden)]
pub mod __private {
    pub use crate::macros::{Empty, Zero, Zeros};
}

pub mod array;
pub mod error;
#[cfg(feature = "std")]
pub mod flags;
#[cfg(feature = "alloc")]
pub mod io;
pub mod strategy;
pub mod traits;

#[cfg(all(feature = "c_raw", not(target_arch = "wasm32")))]
pub use traits::ffi;
pub use traits::wrapper;

pub use array::*;
pub use strategy::Holder;
#[allow(deprecated)]
pub use traits::{AsAny, ByteArray, Dumb, JoinSplit, Wrapper, WrapperMut};

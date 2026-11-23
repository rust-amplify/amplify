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

pub use confined::{ONE, U8MAX, U16MAX, U24MAX, U32MAX, U40MAX, U48MAX, U56MAX, U64MAX, ZERO};

use super::{ConfinedIo, ConfinedReader, ConfinedWriter};
use crate::io::NullIo;

/// [`ConfinedReader`] bounded to `0..=U8MAX` (`0..=255` bytes).
pub type TinyReader<R> = ConfinedReader<R, ZERO, U8MAX>;
/// [`ConfinedReader`] bounded to `0..=U16MAX` (`0..=65_535` bytes).
pub type SmallReader<R> = ConfinedReader<R, ZERO, U16MAX>;
/// [`ConfinedReader`] bounded to `0..=U24MAX` (`0..=16_777_215` bytes).
pub type MediumReader<R> = ConfinedReader<R, ZERO, U24MAX>;
/// [`ConfinedReader`] bounded to `0..=U32MAX` (`0..=4_294_967_295` bytes).
pub type LargeReader<R> = ConfinedReader<R, ZERO, U32MAX>;
/// [`ConfinedReader`] requiring at least `1` byte (`1..=MAX`).
pub type NonEmptyReader<R, const MAX: usize = U64MAX> = ConfinedReader<R, ONE, MAX>;

/// [`ConfinedWriter`] bounded to `0..=U8MAX` (`0..=255` bytes).
pub type TinyWriter<W = NullIo> = ConfinedWriter<W, ZERO, U8MAX>;
/// [`ConfinedWriter`] bounded to `0..=U16MAX` (`0..=65_535` bytes).
pub type SmallWriter<W = NullIo> = ConfinedWriter<W, ZERO, U16MAX>;
/// [`ConfinedWriter`] bounded to `0..=U24MAX` (`0..=16_777_215` bytes).
pub type MediumWriter<W = NullIo> = ConfinedWriter<W, ZERO, U24MAX>;
/// [`ConfinedWriter`] bounded to `0..=U32MAX` (`0..=4_294_967_295` bytes).
pub type LargeWriter<W = NullIo> = ConfinedWriter<W, ZERO, U32MAX>;
/// [`ConfinedWriter`] requiring at least `1` byte (`1..=MAX`).
pub type NonEmptyWriter<W = NullIo, const MAX: usize = U64MAX> = ConfinedWriter<W, ONE, MAX>;

/// [`ConfinedIo`] bounded to `0..=U8MAX` (`0..=255` bytes) in both directions.
pub type TinyIo<S = NullIo> = ConfinedIo<S, ZERO, U8MAX>;
/// [`ConfinedIo`] bounded to `0..=U16MAX` (`0..=65_535` bytes) in both
/// directions.
pub type SmallIo<S = NullIo> = ConfinedIo<S, ZERO, U16MAX>;
/// [`ConfinedIo`] bounded to `0..=U24MAX` (`0..=16_777_215` bytes) in both
/// directions.
pub type MediumIo<S = NullIo> = ConfinedIo<S, ZERO, U24MAX>;
/// [`ConfinedIo`] bounded to `0..=U32MAX` (`0..=4_294_967_295` bytes) in both
/// directions.
pub type LargeIo<S = NullIo> = ConfinedIo<S, ZERO, U32MAX>;
/// [`ConfinedIo`] requiring at least `1` byte (`1..=MAX`) in both directions.
pub type NonEmptyIo<S = NullIo, const MAX: usize = U64MAX> = ConfinedIo<S, ONE, MAX>;

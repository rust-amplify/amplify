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

//! I/O adapters, byte counters, confined streams, stream splitting, and traits
//! for deque-backed buffers.

mod deques;
pub use deques::{AsDequeMut, AsDequeRef};

#[cfg(feature = "std")]
pub mod confined;
#[cfg(feature = "std")]
mod counter;
#[cfg(feature = "std")]
mod cursor_deque;
#[cfg(feature = "std")]
mod split;
#[cfg(feature = "std")]
mod std_io;

#[cfg(feature = "std")]
pub use self::confined::{
    ConfinedIo, ConfinedIoError, ConfinedReader, ConfinedWriter, LargeIo, LargeReader, LargeWriter,
    MediumIo, MediumReader, MediumWriter, NonEmptyIo, NonEmptyReader, NonEmptyWriter, SmallIo,
    SmallReader, SmallWriter, TinyIo, TinyReader, TinyWriter, UnfinishedIoError,
};
#[cfg(feature = "std")]
pub use counter::{IoCounter, NullIo, ReadCounter, WriteCounter};
#[cfg(feature = "std")]
pub use cursor_deque::CursorDeque;
#[cfg(feature = "std")]
pub use split::{IoPair, JoinIoError, SplitIo, SplitIoError, StreamReader, StreamWriter};
#[cfg(feature = "std")]
pub use std_io::IoError;

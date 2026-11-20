//! I/O adapters, stream splitting, and traits for deque-backed buffers.

mod deques;
pub use deques::{AsDequeMut, AsDequeRef};

#[cfg(feature = "std")]
mod cursor_deque;
#[cfg(feature = "std")]
mod split;
#[cfg(feature = "std")]
mod std_io;

#[cfg(feature = "std")]
pub use cursor_deque::CursorDeque;
#[cfg(feature = "std")]
pub use split::{IoPair, JoinIoError, SplitIo, SplitIoError, StreamReader, StreamWriter};
#[cfg(feature = "std")]
pub use std_io::{ConfinedIo, IoError, WriteCounter};

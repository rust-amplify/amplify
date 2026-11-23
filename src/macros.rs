// Rust language amplification library providing multiple generic trait
// implementations, type wrappers, derive macros and other language enhancements
//
// Written in 2019-2020 by
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

use core::marker::PhantomData;
use core::num::{Saturating, Wrapping};

use collections::CollectionBase;
use confined::{Confined, ItemLen};

use crate::Array;
use crate::num::Number;

mod sealed {
    use core::num::{Saturating, Wrapping};

    use collections::CollectionBase;
    use confined::{Confined, ItemLen};

    use super::{
        EmptyCollection, EmptyConfined, ZeroNumber, ZeroSaturating, ZeroWrapping, ZerosArray,
        ZerosNested, ZerosNestedArray, ZerosRaw,
    };
    use crate::Array;
    use crate::num::Number;

    #[diagnostic::on_unimplemented(
        message = "`zero!()` can only construct zero integer values, not `{Self}`",
        label = "not a zero-constructible integer type"
    )]
    pub trait Zero<Kind> {}

    impl<N: Number> Zero<ZeroNumber> for N {}
    impl<T: super::Zero<K>, K> Zero<ZeroWrapping<K>> for Wrapping<T> {}
    impl<T: super::Zero<K>, K> Zero<ZeroSaturating<K>> for Saturating<T> {}

    #[diagnostic::on_unimplemented(
        message = "`zeros!()` can only construct zero-filled integer array types, not `{Self}`",
        label = "not a zero-constructible integer array type"
    )]
    pub trait Zeros<Kind> {}

    impl<T: super::Zero<K>, const N: usize, K> Zeros<ZerosRaw<K>> for [T; N] {}
    impl<T: super::Zeros<K>, const N: usize, K> Zeros<ZerosNested<K>> for [T; N] {}
    impl<T: super::Zero<K>, const LEN: usize, const REVERSE_STR: bool, K> Zeros<ZerosArray<K>>
        for Array<T, LEN, REVERSE_STR>
    {
    }
    impl<T: super::Zeros<K>, const LEN: usize, const REVERSE_STR: bool, K>
        Zeros<ZerosNestedArray<K>> for Array<T, LEN, REVERSE_STR>
    {
    }

    #[diagnostic::on_unimplemented(
        message = "`empty!()` can only construct empty collection types, not `{Self}`",
        label = "not an empty-constructible collection"
    )]
    pub trait Empty<Kind> {}

    impl<C: CollectionBase + Default> Empty<EmptyCollection> for C {}
    impl<C: ItemLen + Default, const MAX_LEN: usize> Empty<EmptyConfined> for Confined<C, 0, MAX_LEN> {}
}

#[doc(hidden)]
pub enum ZeroNumber {}

#[doc(hidden)]
pub struct ZeroWrapping<K>(PhantomData<K>);

#[doc(hidden)]
pub struct ZeroSaturating<K>(PhantomData<K>);

#[doc(hidden)]
#[diagnostic::on_unimplemented(
    message = "`zero!()` can only construct zero integer values, not `{Self}`",
    label = "not a zero-constructible integer type"
)]
pub trait Zero<Kind = ZeroNumber>: sealed::Zero<Kind> + Copy {
    const ZERO: Self;
}

impl<N: Number> Zero<ZeroNumber> for N {
    const ZERO: Self = N::ZERO;
}

impl<T: Zero<K>, K> Zero<ZeroWrapping<K>> for Wrapping<T> {
    const ZERO: Self = Self(T::ZERO);
}

impl<T: Zero<K>, K> Zero<ZeroSaturating<K>> for Saturating<T> {
    const ZERO: Self = Self(T::ZERO);
}

#[doc(hidden)]
pub struct ZerosRaw<K>(PhantomData<K>);

#[doc(hidden)]
pub struct ZerosNested<K>(PhantomData<K>);

#[doc(hidden)]
pub struct ZerosArray<K>(PhantomData<K>);

#[doc(hidden)]
pub struct ZerosNestedArray<K>(PhantomData<K>);

#[doc(hidden)]
#[diagnostic::on_unimplemented(
    message = "`zeros!()` can only construct zero-filled integer array types, not `{Self}`",
    label = "not a zero-constructible integer array type"
)]
pub trait Zeros<Kind = ZerosRaw<ZeroNumber>>: sealed::Zeros<Kind> + Copy {
    const ZEROS: Self;
}

impl<T: Zero<K>, const N: usize, K> Zeros<ZerosRaw<K>> for [T; N] {
    const ZEROS: Self = [T::ZERO; N];
}

impl<T: Zeros<K>, const N: usize, K> Zeros<ZerosNested<K>> for [T; N] {
    const ZEROS: Self = [T::ZEROS; N];
}

impl<T: Zero<K>, const LEN: usize, const REVERSE_STR: bool, K> Zeros<ZerosArray<K>>
    for Array<T, LEN, REVERSE_STR>
{
    const ZEROS: Self = Self::from_array([T::ZERO; LEN]);
}

impl<T: Zeros<K>, const LEN: usize, const REVERSE_STR: bool, K> Zeros<ZerosNestedArray<K>>
    for Array<T, LEN, REVERSE_STR>
{
    const ZEROS: Self = Self::from_array([T::ZEROS; LEN]);
}

#[doc(hidden)]
pub enum EmptyCollection {}

#[doc(hidden)]
pub enum EmptyConfined {}

#[doc(hidden)]
#[diagnostic::on_unimplemented(
    message = "`empty!()` can only construct empty collection types, not `{Self}`",
    label = "not an empty-constructible collection"
)]
pub trait Empty<Kind = EmptyCollection>: sealed::Empty<Kind> {
    fn empty() -> Self;
}

impl<C: CollectionBase + Default> Empty<EmptyCollection> for C {
    #[inline]
    fn empty() -> Self {
        Self::default()
    }
}

impl<C: ItemLen + Default, const MAX_LEN: usize> Empty<EmptyConfined> for Confined<C, 0, MAX_LEN> {
    #[inline]
    fn empty() -> Self {
        Self::new()
    }
}

/// This macro allows more semantically-clear code (which can be used especially
/// with structure initialization), indicating that instead of type value we are
/// generating zero values (applies to integer numeric types).
///
/// Works (including in `const` contexts) for all integer types implementing
/// [`Number`](crate::num::Number) — primitive signed/unsigned integers,
/// custom-width small unsigned integers (`u1`..`u56`), and
/// [`BigInt`](crate::num::BigInt) types (`u256`..`u1024`, `i256`..`i1024`) — as
/// well as [`Wrapping`] and [`Saturating`] over them:
///
/// ```
/// use core::num::{Saturating, Wrapping};
/// use amplify::num::{i256, u4, u24, u256};
/// use amplify::zero;
///
/// const ZERO_U32: u32 = zero!();
/// const ZERO_U256: u256 = zero!();
/// const ZERO_WRAP: Wrapping<i64> = zero!();
/// const ZERO_SAT: Saturating<u24> = zero!();
///
/// let a: u8 = zero!();
/// let b: i128 = zero!();
/// let c: usize = zero!();
/// let d: u4 = zero!();
/// let e: i256 = zero!();
///
/// assert_eq!(ZERO_U32, 0);
/// assert_eq!(ZERO_U256, u256::ZERO);
/// assert_eq!(ZERO_WRAP, Wrapping(0));
/// assert_eq!(ZERO_SAT, Saturating(u24::ZERO));
/// assert_eq!((a, b, c, d, e), (0, 0, 0, u4::ZERO, i256::ZERO));
/// ```
///
/// Floating-point, nonzero, and non-numeric types are rejected at compile time:
///
/// ```compile_fail,E0277
/// let _: f32 = amplify::zero!();
/// ```
///
/// ```compile_fail,E0277
/// let _: f64 = amplify::zero!();
/// ```
///
/// ```compile_fail,E0277
/// let _: core::num::Wrapping<f64> = amplify::zero!();
/// ```
///
/// ```compile_fail,E0277
/// let _: core::num::NonZeroU8 = amplify::zero!();
/// ```
///
/// ```compile_fail,E0277
/// let _: bool = amplify::zero!();
/// ```
///
/// ```compile_fail,E0277
/// let _: Option<u8> = amplify::zero!();
/// ```
///
/// ```compile_fail,E0277
/// let _: [u8; 4] = amplify::zero!();
/// ```
///
/// ```compile_fail,E0277
/// let _: amplify::Bytes32 = amplify::zero!();
/// ```
///
/// ```compile_fail,E0277
/// #[derive(Copy, Clone, Default)]
/// struct NotANumber;
///
/// let _: NotANumber = amplify::zero!();
/// ```
///
/// Downstream crates cannot implement the sealed helper trait for custom types:
///
/// ```compile_fail,E0277
/// #[derive(Copy, Clone)]
/// struct Custom;
///
/// impl amplify::__private::Zero for Custom {
///     const ZERO: Self = Custom;
/// }
/// ```
#[macro_export]
macro_rules! zero {
    () => {
        <_ as $crate::__private::Zero<_>>::ZERO
    };
}

/// This macro allows more semantically-clear code (which can be used especially
/// with structure initialization), indicating that instead of type value we are
/// generating zero values (applies to arrays).
///
/// Works (including in `const` contexts) for single- and multi-dimensional
/// `[T; N]` arrays of any length and [`Array`] / [`Bytes`](crate::Bytes)
/// wrappers (`Bytes4`..`Bytes64`, `Bytes32StrRev`) over zero-constructible
/// integer types:
///
/// ```
/// use core::num::{Saturating, Wrapping};
/// use amplify::num::{u24, u256};
/// use amplify::{Array, Bytes32, Bytes32StrRev, zeros};
///
/// const EMPTY_ARR: [u8; 0] = zeros!();
/// const LARGE_ARR: [u8; 64] = zeros!();
/// const MATRIX: [[u16; 3]; 2] = zeros!();
/// const HASH: Bytes32 = zeros!();
/// const REV_HASH: Bytes32StrRev = zeros!();
///
/// let small_ints: [u24; 4] = zeros!();
/// let big_ints: [u256; 2] = zeros!();
/// let wrapped: [Wrapping<u32>; 2] = zeros!();
/// let saturated: [Saturating<i16>; 2] = zeros!();
/// let amp_arr: Array<i64, 4> = zeros!();
/// let nested_amp: [Bytes32; 2] = zeros!();
///
/// assert!(EMPTY_ARR.is_empty());
/// assert_eq!(LARGE_ARR, [0; 64]);
/// assert_eq!(MATRIX, [[0; 3]; 2]);
/// assert!(HASH == Bytes32::zero());
/// assert!(REV_HASH == Bytes32StrRev::zero());
/// assert_eq!(small_ints, [u24::ZERO; 4]);
/// assert_eq!(big_ints, [u256::ZERO; 2]);
/// assert_eq!(wrapped, [Wrapping(0); 2]);
/// assert_eq!(saturated, [Saturating(0); 2]);
/// assert!(amp_arr == Array::from_array([0; 4]));
/// assert!(nested_amp == [Bytes32::zero(); 2]);
/// ```
///
/// Non-arrays, floating-point arrays, and arrays of non-integer types are
/// rejected at compile time:
///
/// ```compile_fail,E0277
/// let _: u32 = amplify::zeros!();
/// ```
///
/// ```compile_fail,E0277
/// let _: [f32; 4] = amplify::zeros!();
/// ```
///
/// ```compile_fail,E0277
/// let _: amplify::Array<f64, 4> = amplify::zeros!();
/// ```
///
/// ```compile_fail,E0277
/// let _: [bool; 4] = amplify::zeros!();
/// ```
///
/// ```compile_fail,E0277
/// let _: [Option<u8>; 4] = amplify::zeros!();
/// ```
///
/// ```compile_fail,E0277
/// let _: [core::num::NonZeroU8; 4] = amplify::zeros!();
/// ```
///
/// ```compile_fail,E0277
/// #[derive(Copy, Clone, Default)]
/// struct NotANumber;
///
/// let _: [NotANumber; 4] = amplify::zeros!();
/// ```
///
/// Downstream crates cannot implement the sealed helper trait for custom types:
///
/// ```compile_fail,E0277
/// #[derive(Copy, Clone)]
/// struct Custom;
///
/// impl amplify::__private::Zeros for Custom {
///     const ZEROS: Self = Custom;
/// }
/// ```
#[macro_export]
macro_rules! zeros {
    () => {
        <_ as $crate::__private::Zeros<_>>::ZEROS
    };
}

/// This macro allows more semantically-clear code (which can be used especially
/// with structure initialization), indicating that instead of type value we are
/// generating empty collection types.
///
/// Works for all unconfined collections implementing
/// [`CollectionBase`](collections::CollectionBase) + [`Default`] and all
/// zero-min-bound [`Confined`](confined::Confined) collections:
///
/// ```
/// use amplify::empty;
///
/// # #[cfg(feature = "alloc")]
/// # {
/// use std::collections::{BTreeMap, BTreeSet, BinaryHeap, LinkedList, VecDeque};
/// use amplify::confined::{TinyBlob, TinyOrdMap, TinyOrdSet, TinyString, TinyVec};
///
/// let v: Vec<u8> = empty!();
/// let s: String = empty!();
/// let d: VecDeque<u8> = empty!();
/// let l: LinkedList<u8> = empty!();
/// let bs: BTreeSet<u8> = empty!();
/// let bm: BTreeMap<u8, u8> = empty!();
/// let bh: BinaryHeap<u8> = empty!();
/// assert!(v.is_empty() && s.is_empty() && d.is_empty() && l.is_empty());
/// assert!(bs.is_empty() && bm.is_empty() && bh.is_empty());
///
/// let tv: TinyVec<u8> = empty!();
/// let tb: TinyBlob = empty!();
/// let ts: TinyString = empty!();
/// let tos: TinyOrdSet<u8> = empty!();
/// let tom: TinyOrdMap<u8, u8> = empty!();
/// assert!(tv.is_empty() && tb.is_empty() && ts.is_empty() && tos.is_empty() && tom.is_empty());
/// # }
/// # #[cfg(feature = "std")]
/// # {
/// use std::collections::{HashMap, HashSet};
/// use amplify::confined::{TinyHashMap, TinyHashSet};
///
/// let hs: HashSet<u8> = empty!();
/// let hm: HashMap<u8, u8> = empty!();
/// let ths: TinyHashSet<u8> = empty!();
/// let thm: TinyHashMap<u8, u8> = empty!();
/// assert!(hs.is_empty() && hm.is_empty() && ths.is_empty() && thm.is_empty());
/// # }
/// # #[cfg(feature = "ascii")]
/// # {
/// use amplify::ascii::AsciiString;
/// use amplify::confined::TinyAscii;
///
/// let a: AsciiString = empty!();
/// let ta: TinyAscii = empty!();
/// assert!(a.is_empty() && ta.is_empty());
/// # }
/// # #[cfg(feature = "hashbrown")]
/// # {
/// use amplify::collections::hashbrown::{HashMap as HbMap, HashSet as HbSet};
/// use amplify::confined::{TinyHashbrownMap, TinyHashbrownSet};
///
/// let hs: HbSet<u8> = empty!();
/// let hm: HbMap<u8, u8> = empty!();
/// let ths: TinyHashbrownSet<u8> = empty!();
/// let thm: TinyHashbrownMap<u8, u8> = empty!();
/// assert!(hs.is_empty() && hm.is_empty() && ths.is_empty() && thm.is_empty());
/// # }
/// # #[cfg(all(feature = "indexmap", feature = "std"))]
/// # {
/// use amplify::collections::indexmap::{IndexMap, IndexSet};
/// use amplify::confined::{TinyIndexMap, TinyIndexSet};
///
/// let is: IndexSet<u8> = empty!();
/// let im: IndexMap<u8, u8> = empty!();
/// let tis: TinyIndexSet<u8> = empty!();
/// let tim: TinyIndexMap<u8, u8> = empty!();
/// assert!(is.is_empty() && im.is_empty() && tis.is_empty() && tim.is_empty());
/// # }
/// # #[cfg(feature = "heapless")]
/// # {
/// use amplify::collections::heapless::{Deque, LinearMap, String as HString, Vec as HVec};
/// use amplify::confined::{HeaplessDeque, HeaplessLinearMap, HeaplessString, HeaplessVec};
///
/// let hv: HVec<u8, 4> = empty!();
/// let hs: HString<8> = empty!();
/// let hd: Deque<u8, 4> = empty!();
/// let hlm: LinearMap<u8, u8, 4> = empty!();
/// let chv: HeaplessVec<u8, 4> = empty!();
/// let chs: HeaplessString<8> = empty!();
/// let chd: HeaplessDeque<u8, 4> = empty!();
/// let chlm: HeaplessLinearMap<u8, u8, 4> = empty!();
/// assert!(hv.is_empty() && hs.is_empty() && hd.is_empty() && hlm.is_empty());
/// assert!(chv.is_empty() && chs.is_empty() && chd.is_empty() && chlm.is_empty());
/// # }
/// ```
///
/// Non-collection types are rejected at compile time:
///
/// ```compile_fail,E0277
/// let _: u32 = amplify::empty!();
/// ```
///
/// ```compile_fail,E0277
/// let _: Option<u8> = amplify::empty!();
/// ```
///
/// ```compile_fail,E0277
/// let _: [u8; 0] = amplify::empty!();
/// ```
///
/// ```compile_fail,E0277
/// let _: amplify::Bytes32 = amplify::empty!();
/// ```
///
/// ```compile_fail,E0277
/// #[derive(Default)]
/// struct NotACollection;
///
/// let _: NotACollection = amplify::empty!();
/// ```
///
/// Non-empty confined collections (`MIN_LEN > 0`) are also rejected:
///
/// ```compile_fail,E0277
/// # #[derive(Default)]
/// # struct Dummy;
/// # impl amplify::collections::CollectionBase for Dummy {
/// #     type Item = u8;
/// #     fn item_count(&self) -> usize { 0 }
/// #     fn push(&mut self, _: u8) {}
/// #     fn clear(&mut self) {}
/// # }
/// # impl amplify::confined::ItemLen for Dummy {}
/// let _: amplify::confined::Confined<Dummy, 1, 4> = amplify::empty!();
/// ```
///
/// Downstream crates cannot implement the sealed helper trait for custom types:
///
/// ```compile_fail,E0277
/// struct Custom;
///
/// impl amplify::__private::Empty for Custom {
///     fn empty() -> Self { Custom }
/// }
/// ```
///
/// ```compile_fail,E0277
/// struct Custom;
/// enum CustomKind {}
///
/// impl amplify::__private::Empty<CustomKind> for Custom {
///     fn empty() -> Self { Custom }
/// }
/// ```
#[macro_export]
macro_rules! empty {
    () => {
        <_ as $crate::__private::Empty<_>>::empty()
    };
}

/// Shorthand for [`Default::default()`].
///
/// Works for any type implementing [`Default`]:
///
/// ```
/// use amplify::{Bytes64, default};
///
/// #[derive(Debug, PartialEq, Eq, Default)]
/// struct Config {
///     enabled: bool,
///     tag: Option<u8>,
/// }
///
/// let n: u16 = default!();
/// let f: f64 = default!();
/// let opt: Option<u8> = default!();
/// let arr: [u8; 4] = default!();
/// let bytes: Bytes64 = default!();
/// let cfg: Config = default!();
///
/// assert_eq!(n, 0);
/// assert_eq!(f, 0.0);
/// assert_eq!(opt, None);
/// assert_eq!(arr, [0; 4]);
/// assert!(bytes == Bytes64::zero());
/// assert_eq!(
///     cfg,
///     Config {
///         enabled: false,
///         tag: None
///     }
/// );
/// ```
///
/// Types that do not implement [`Default`] are rejected at compile time:
///
/// ```compile_fail,E0277
/// let _: core::num::NonZeroU8 = amplify::default!();
/// ```
///
/// ```compile_fail,E0277
/// let _: [u8; 33] = amplify::default!();
/// ```
///
/// ```compile_fail,E0277
/// struct NoDefault;
///
/// let _: NoDefault = amplify::default!();
/// ```
///
/// ```compile_fail,E0277
/// # #[derive(Default)]
/// # struct Dummy;
/// # impl amplify::collections::CollectionBase for Dummy {
/// #     type Item = u8;
/// #     fn item_count(&self) -> usize { 0 }
/// #     fn push(&mut self, _: u8) {}
/// #     fn clear(&mut self) {}
/// # }
/// # impl amplify::confined::ItemLen for Dummy {}
/// let _: amplify::confined::Confined<Dummy, 1, 4> = amplify::default!();
/// ```
#[macro_export]
macro_rules! default {
    () => {
        ::core::default::Default::default()
    };
}

/// Shorthand for [`Dumb::dumb()`](crate::Dumb::dumb).
///
/// Works only for types implementing [`Dumb`](crate::Dumb):
///
/// ```
/// #![allow(deprecated)]
/// use amplify::{Bytes4, Dumb, dumb};
///
/// #[derive(Debug, PartialEq, Eq)]
/// struct Fixture(u8);
///
/// impl Dumb for Fixture {
///     fn dumb() -> Self {
///         Self(42)
///     }
/// }
///
/// let byte: u8 = dumb!();
/// let bytes: Bytes4 = dumb!();
/// let fixture: Fixture = dumb!();
///
/// let _ = (byte, bytes);
/// assert_eq!(fixture, Fixture(42));
/// ```
///
/// Types not implementing [`Dumb`](crate::Dumb) are rejected at compile time:
///
/// ```compile_fail,E0277
/// #![allow(deprecated)]
/// let _: u32 = amplify::dumb!();
/// ```
///
/// ```compile_fail,E0277
/// #![allow(deprecated)]
/// let _: Option<u8> = amplify::dumb!();
/// ```
///
/// ```compile_fail,E0277
/// #![allow(deprecated)]
/// #[derive(Default)]
/// struct NotDumb;
///
/// let _: NotDumb = amplify::dumb!();
/// ```
#[macro_export]
#[deprecated(note = "this test helper has limited implementations and may be removed")]
macro_rules! dumb {
    () => {
        $crate::Dumb::dumb()
    };
}

#[macro_use]
extern crate amplify;

#[cfg(feature = "alloc")]
extern crate alloc;

#[cfg(feature = "alloc")]
#[test]
fn confined_constructors_are_available_by_path_and_macro_use() {
    let by_path = amplify::tiny_vec![1u8, 2];
    let by_macro_use = tiny_vec![3u8, 4];

    assert_eq!(by_path.as_slice(), &[1, 2]);
    assert_eq!(by_macro_use.as_slice(), &[3, 4]);
}

#[cfg(feature = "alloc")]
#[test]
fn collection_alloc_macros_are_reexported_by_path_and_macro_use() {
    use alloc::collections::{BTreeMap, BTreeSet, BinaryHeap, LinkedList, VecDeque};
    use alloc::string::String;

    let owned = s!("Amplify");
    let by_path = amplify::s!("Facade");
    assert_eq!(owned, "Amplify");
    assert_eq!(by_path, "Facade");

    let empty_bmap: BTreeMap<u8, u8> = amplify::bmap! {};
    assert!(empty_bmap.is_empty());
    let map: BTreeMap<String, String> = bmap! { owned: "key" => "value" };
    assert_eq!(map["key"], "value");

    let empty_bset: BTreeSet<u8> = amplify::bset![];
    assert!(empty_bset.is_empty());
    let set: BTreeSet<String> = bset! { owned: "one", "two", "one" };
    assert_eq!(set.len(), 2);

    let empty_list: LinkedList<u8> = amplify::list! {};
    assert!(empty_list.is_empty());
    let items: LinkedList<String> = list! { owned: "first" => "second" };
    assert_eq!(items.front().map(String::as_str), Some("first"));

    let empty_deque: VecDeque<u8> = amplify::deque! {};
    assert!(empty_deque.is_empty());
    let queue: VecDeque<String> = deque! { owned: "first" => "second" };
    assert_eq!(queue.front().map(String::as_str), Some("first"));

    let empty_heap: BinaryHeap<u8> = amplify::heap![];
    assert!(empty_heap.is_empty());
    let prio = heap![1u8, 3, 2];
    assert_eq!(prio.peek(), Some(&3));
}

#[cfg(feature = "std")]
#[test]
fn collection_std_macros_are_reexported_by_path_and_macro_use() {
    use std::collections::{HashMap, HashSet};

    let empty_map: HashMap<u8, u8> = amplify::map! {};
    assert!(empty_map.is_empty());
    let hashed_map: HashMap<String, String> = map! { owned: "key" => "value" };
    assert_eq!(hashed_map["key"], "value");

    let empty_set: HashSet<u8> = amplify::set![];
    assert!(empty_set.is_empty());
    let hashed_set: HashSet<String> = set! { owned: "one", "two", "one" };
    assert_eq!(hashed_set.len(), 2);
}

#[test]
fn local_default_macros_expand_in_downstream_crates() {
    #[derive(Default)]
    struct DummyCollection(usize);

    impl amplify::collections::CollectionBase for DummyCollection {
        type Item = u8;

        fn item_count(&self) -> usize {
            self.0
        }

        fn push(&mut self, _: u8) {
            self.0 += 1;
        }

        fn clear(&mut self) {
            self.0 = 0;
        }
    }

    impl amplify::confined::ItemLen for DummyCollection {}

    let zeroed: u32 = zero!();
    let zero_by_path: u64 = amplify::zero!();
    let zero_arr: [u8; 4] = zeros!();
    let zero_bytes: amplify::Bytes32 = amplify::zeros!();
    let empty_custom: DummyCollection = empty!();
    let empty_confined: amplify::confined::Confined<DummyCollection, 0, 4> = amplify::empty!();
    let fallback_local: Option<u8> = default!();
    let fallback: u16 = amplify::default!();
    #[allow(deprecated)]
    let (dumb_local, dumb_by_path): (u8, amplify::Bytes4) = (dumb!(), amplify::dumb!());

    assert_eq!(zeroed, 0);
    assert_eq!(zero_by_path, 0);
    assert_eq!(zero_arr, [0; 4]);
    assert!(zero_bytes == amplify::Bytes32::zero());
    assert_eq!(empty_custom.0, 0);
    assert!(empty_confined.is_empty());
    assert_eq!(fallback_local, None);
    assert_eq!(fallback, 0);
    let _ = (dumb_local, dumb_by_path);
}

#[test]
fn zero_macro_supports_all_integers_and_wrappers() {
    use core::num::{Saturating, Wrapping};
    use amplify::num::{
        BigInt, Number, i256, i512, i1024, u1, u2, u3, u4, u5, u6, u7, u10, u12, u14, u20, u24,
        u40, u48, u56, u256, u512, u1024,
    };

    const C_U32: u32 = zero!();
    const C_I128: i128 = amplify::zero!();
    const C_U24: u24 = zero!();
    const C_U256: u256 = zero!();
    const C_I512: i512 = zero!();
    const C_WRAP: Wrapping<u64> = zero!();
    const C_SAT: Saturating<i32> = zero!();

    fn generic_zero<N: Number>() -> N {
        zero!()
    }
    fn generic_wrapping<N: Number>() -> Wrapping<N> {
        zero!()
    }
    fn generic_saturating<N: Number>() -> Saturating<N> {
        zero!()
    }

    assert_eq!(C_U32, 0);
    assert_eq!(C_I128, 0);
    assert_eq!(C_U24, u24::ZERO);
    assert_eq!(C_U256, u256::ZERO);
    assert_eq!(C_I512, i512::ZERO);
    assert_eq!(C_WRAP, Wrapping(0));
    assert_eq!(C_SAT, Saturating(0));

    let prim_u: (u8, u16, u32, u64, u128, usize) =
        (zero!(), zero!(), zero!(), zero!(), zero!(), zero!());
    let prim_i: (i8, i16, i32, i64, i128, isize) =
        (zero!(), zero!(), zero!(), zero!(), zero!(), zero!());
    assert_eq!(prim_u, (0, 0, 0, 0, 0, 0));
    assert_eq!(prim_i, (0, 0, 0, 0, 0, 0));

    let small_a: (u1, u2, u3, u4, u5, u6, u7) = (
        zero!(),
        zero!(),
        zero!(),
        zero!(),
        zero!(),
        zero!(),
        zero!(),
    );
    let small_b: (u10, u12, u14, u20, u24, u40, u48, u56) = (
        zero!(),
        zero!(),
        zero!(),
        zero!(),
        zero!(),
        zero!(),
        zero!(),
        zero!(),
    );
    assert_eq!(
        small_a,
        (
            u1::ZERO,
            u2::ZERO,
            u3::ZERO,
            u4::ZERO,
            u5::ZERO,
            u6::ZERO,
            u7::ZERO
        )
    );
    assert_eq!(
        small_b,
        (
            u10::ZERO,
            u12::ZERO,
            u14::ZERO,
            u20::ZERO,
            u24::ZERO,
            u40::ZERO,
            u48::ZERO,
            u56::ZERO
        )
    );

    let big_u: (u256, u512, u1024, BigInt<2, false>) = (zero!(), zero!(), zero!(), zero!());
    let big_i: (i256, i512, i1024, BigInt<2, true>) = (zero!(), zero!(), zero!(), zero!());
    assert_eq!(big_u, (u256::ZERO, u512::ZERO, u1024::ZERO, BigInt::ZERO));
    assert_eq!(big_i, (i256::ZERO, i512::ZERO, i1024::ZERO, BigInt::ZERO));

    let wrapped: (Wrapping<u8>, Wrapping<i64>, Wrapping<u24>, Wrapping<u256>) =
        (zero!(), zero!(), zero!(), zero!());
    let nested_wrap: Wrapping<Wrapping<u16>> = zero!();
    let saturated: (
        Saturating<u16>,
        Saturating<isize>,
        Saturating<u48>,
        Saturating<i256>,
    ) = (zero!(), zero!(), zero!(), zero!());
    assert_eq!(
        wrapped,
        (
            Wrapping(0),
            Wrapping(0),
            Wrapping(u24::ZERO),
            Wrapping(u256::ZERO),
        )
    );
    assert_eq!(nested_wrap, Wrapping(Wrapping(0)));
    assert_eq!(
        saturated,
        (
            Saturating(0),
            Saturating(0),
            Saturating(u48::ZERO),
            Saturating(i256::ZERO)
        )
    );

    assert_eq!(generic_zero::<u24>(), u24::ZERO);
    assert_eq!(generic_wrapping::<i256>(), Wrapping(i256::ZERO));
    assert_eq!(generic_saturating::<u16>(), Saturating(0));
}

#[test]
fn zeros_macro_supports_raw_nested_and_amplify_arrays() {
    use core::num::{Saturating, Wrapping};
    use amplify::num::{Number, i256, u24, u256};
    use amplify::{Array, Bytes, Bytes4, Bytes16, Bytes20, Bytes32, Bytes32StrRev, Bytes64, Wrapper};

    const EMPTY_RAW: [u8; 0] = zeros!();
    const LARGE_RAW: [u8; 64] = zeros!();
    const MATRIX: [[u16; 3]; 2] = amplify::zeros!();
    const CUBE: [[[i32; 2]; 2]; 2] = zeros!();
    const HASH: Bytes32 = zeros!();
    const REV_HASH: Bytes32StrRev = zeros!();

    fn generic_raw<N: Number, const LEN: usize>() -> [N; LEN] {
        zeros!()
    }
    fn generic_nested<N: Number, const M: usize, const LEN: usize>() -> [[N; M]; LEN] {
        zeros!()
    }
    fn generic_amp<N: Number, const LEN: usize>() -> Array<N, LEN> {
        zeros!()
    }

    assert!(EMPTY_RAW.is_empty());
    assert_eq!(LARGE_RAW, [0; 64]);
    assert_eq!(MATRIX, [[0; 3]; 2]);
    assert_eq!(CUBE, [[[0; 2]; 2]; 2]);
    assert!(HASH == Bytes32::zero());
    assert!(REV_HASH == Bytes32StrRev::zero());

    let small_arr: [u24; 3] = zeros!();
    let big_arr: [u256; 2] = zeros!();
    let signed_big_arr: [i256; 2] = zeros!();
    let wrap_arr: [Wrapping<u32>; 2] = zeros!();
    let sat_arr: [Saturating<i16>; 2] = zeros!();
    assert_eq!(small_arr, [u24::ZERO; 3]);
    assert_eq!(big_arr, [u256::ZERO; 2]);
    assert_eq!(signed_big_arr, [i256::ZERO; 2]);
    assert_eq!(wrap_arr, [Wrapping(0); 2]);
    assert_eq!(sat_arr, [Saturating(0); 2]);

    let bytes_custom: Bytes<12> = zeros!();
    let b4: Bytes4 = zeros!();
    let b16: Bytes16 = zeros!();
    let b20: Bytes20 = zeros!();
    let b64: Bytes64 = zeros!();
    let typed_amp: Array<u24, 4> = zeros!();
    let nested_amp: [Bytes4; 2] = zeros!();
    let amp_of_raw: Array<[u8; 4], 2> = zeros!();
    assert_eq!(bytes_custom.into_inner(), [0; 12]);
    assert!(b4 == Bytes4::zero());
    assert!(b16 == Bytes16::zero());
    assert!(b20 == Bytes20::zero());
    assert!(b64 == Bytes64::zero());
    assert_eq!(typed_amp.into_inner(), [u24::ZERO; 4]);
    assert!(nested_amp == [Bytes4::zero(); 2]);
    assert_eq!(amp_of_raw.into_inner(), [[0; 4]; 2]);

    let mut inferred_raw: [_; 4] = zeros!();
    inferred_raw[0] = 9u32;
    assert_eq!(inferred_raw, [9, 0, 0, 0]);

    let mut inferred_amp: Array<_, 3> = zeros!();
    inferred_amp[0] = 5i64;
    assert_eq!(inferred_amp.into_inner(), [5, 0, 0]);

    assert_eq!(generic_raw::<u24, 2>(), [u24::ZERO; 2]);
    assert_eq!(generic_nested::<u8, 2, 3>(), [[0; 2]; 3]);
    assert_eq!(generic_amp::<i256, 2>().into_inner(), [i256::ZERO; 2]);
}

#[test]
#[allow(deprecated)]
fn default_and_dumb_macros_support_expected_types() {
    use amplify::{Array, Bytes32, Bytes64, Dumb};

    #[derive(Debug, PartialEq, Eq, Default)]
    struct Sample {
        flag: bool,
        note: Option<u8>,
    }

    #[derive(Debug, PartialEq, Eq)]
    struct CustomDumb(u16);

    impl Dumb for CustomDumb {
        fn dumb() -> Self {
            Self(0xCAFE)
        }
    }

    let b: bool = default!();
    let f: f64 = default!();
    let opt: Option<u32> = default!();
    let arr32: [u8; 32] = default!();
    let bytes64: Bytes64 = default!();
    let sample: Sample = amplify::default!();

    assert!(!b);
    assert_eq!(f, 0.0);
    assert_eq!(opt, None);
    assert_eq!(arr32, [0; 32]);
    assert!(bytes64 == Bytes64::zero());
    assert_eq!(
        sample,
        Sample {
            flag: false,
            note: None
        }
    );

    #[cfg(feature = "std")]
    {
        let flags: amplify::flags::FlagVec = default!();
        assert!(!flags.is_set(0u16));
    }

    let dumb_u8: u8 = dumb!();
    let dumb_bytes: Bytes32 = dumb!();
    let dumb_arr: Array<u8, 3> = amplify::dumb!();
    let dumb_custom: CustomDumb = dumb!();

    let _ = (dumb_u8, dumb_bytes, dumb_arr);
    assert_eq!(dumb_custom, CustomDumb(0xCAFE));
}

#[cfg(feature = "alloc")]
#[test]
fn empty_macro_supports_alloc_and_confined_collections() {
    use alloc::collections::{BTreeMap, BTreeSet, BinaryHeap, LinkedList, VecDeque};
    use alloc::string::String;
    use alloc::vec::Vec;
    use amplify::confined::{
        TinyBlob, TinyDeque, TinyHeap, TinyList, TinyOrdMap, TinyOrdSet, TinyString, TinyVec,
    };

    let s: String = empty!();
    let v: Vec<u8> = empty!();
    let d: VecDeque<u8> = empty!();
    let l: LinkedList<u8> = empty!();
    let bs: BTreeSet<u8> = empty!();
    let bm: BTreeMap<u8, u8> = empty!();
    let bh: BinaryHeap<u8> = empty!();

    assert!(s.is_empty());
    assert!(v.is_empty());
    assert!(d.is_empty());
    assert!(l.is_empty());
    assert!(bs.is_empty());
    assert!(bm.is_empty());
    assert!(bh.is_empty());

    let ts: TinyString = empty!();
    let tb: TinyBlob = empty!();
    let tv: TinyVec<u8> = empty!();
    let td: TinyDeque<u8> = empty!();
    let tl: TinyList<u8> = empty!();
    let th: TinyHeap<u8> = empty!();
    let tos: TinyOrdSet<u8> = empty!();
    let tom: TinyOrdMap<u8, u8> = empty!();

    assert!(ts.is_empty());
    assert!(tb.is_empty());
    assert!(tv.is_empty());
    assert!(td.is_empty());
    assert!(tl.is_empty());
    assert!(th.is_empty());
    assert!(tos.is_empty());
    assert!(tom.is_empty());
}

#[cfg(feature = "std")]
#[test]
fn empty_macro_supports_std_collections() {
    use std::collections::{HashMap, HashSet};
    use amplify::confined::{TinyHashMap, TinyHashSet};

    let hs: HashSet<u8> = empty!();
    let hm: HashMap<u8, u8> = empty!();
    let ths: TinyHashSet<u8> = empty!();
    let thm: TinyHashMap<u8, u8> = empty!();

    assert!(hs.is_empty());
    assert!(hm.is_empty());
    assert!(ths.is_empty());
    assert!(thm.is_empty());
}

#[cfg(feature = "ascii")]
#[test]
fn empty_macro_supports_ascii_collections() {
    use amplify::ascii::AsciiString;
    use amplify::confined::TinyAscii;

    let a: AsciiString = empty!();
    let ta: TinyAscii = empty!();

    assert!(a.is_empty());
    assert!(ta.is_empty());
}

#[cfg(feature = "hashbrown")]
#[test]
fn empty_macro_supports_hashbrown_collections() {
    use amplify::collections::hashbrown::{HashMap, HashSet};
    use amplify::confined::{TinyHashbrownMap, TinyHashbrownSet};

    let hs: HashSet<u8> = empty!();
    let hm: HashMap<u8, u8> = empty!();
    let ths: TinyHashbrownSet<u8> = empty!();
    let thm: TinyHashbrownMap<u8, u8> = empty!();

    assert!(hs.is_empty());
    assert!(hm.is_empty());
    assert!(ths.is_empty());
    assert!(thm.is_empty());
}

#[cfg(feature = "indexmap")]
#[test]
fn empty_macro_supports_indexmap_collections() {
    use core::hash::{BuildHasherDefault, Hasher};
    use amplify::collections::indexmap::{IndexMap, IndexSet};
    use amplify::confined::Confined;

    #[derive(Default)]
    struct TestHasher(u64);
    impl Hasher for TestHasher {
        fn finish(&self) -> u64 {
            self.0
        }
        fn write(&mut self, bytes: &[u8]) {
            for &b in bytes {
                self.0 = self.0.wrapping_mul(31).wrapping_add(u64::from(b));
            }
        }
    }
    type S = BuildHasherDefault<TestHasher>;

    let is: IndexSet<u8, S> = empty!();
    let im: IndexMap<u8, u8, S> = empty!();
    let cis: Confined<IndexSet<u8, S>, 0, 8> = empty!();
    let cim: Confined<IndexMap<u8, u8, S>, 0, 8> = empty!();

    assert!(is.is_empty());
    assert!(im.is_empty());
    assert!(cis.is_empty());
    assert!(cim.is_empty());

    #[cfg(feature = "std")]
    {
        use amplify::confined::{TinyIndexMap, TinyIndexSet};

        let tis: TinyIndexSet<u8> = empty!();
        let tim: TinyIndexMap<u8, u8> = empty!();
        assert!(tis.is_empty());
        assert!(tim.is_empty());
    }
}

#[cfg(feature = "heapless")]
#[test]
fn empty_macro_supports_heapless_collections() {
    use core::hash::{BuildHasherDefault, Hasher};
    use amplify::collections::heapless::binary_heap::Max;
    use amplify::collections::heapless::{
        BinaryHeap, Deque, IndexMap, IndexSet, LinearMap, String, Vec,
    };
    use amplify::confined::{
        HeaplessDeque, HeaplessHeap, HeaplessIndexMap, HeaplessIndexSet, HeaplessLinearMap,
        HeaplessString, HeaplessVec,
    };

    #[derive(Default)]
    struct TestHasher(u64);
    impl Hasher for TestHasher {
        fn finish(&self) -> u64 {
            self.0
        }
        fn write(&mut self, bytes: &[u8]) {
            for &b in bytes {
                self.0 = self.0.wrapping_mul(31).wrapping_add(u64::from(b));
            }
        }
    }
    type S = BuildHasherDefault<TestHasher>;

    let v: Vec<u8, 4> = empty!();
    let s: String<8> = empty!();
    let d: Deque<u8, 4> = empty!();
    let bh: BinaryHeap<u8, Max, 4> = empty!();
    let lm: LinearMap<u8, u8, 4> = empty!();
    let is: IndexSet<u8, S, 4> = empty!();
    let im: IndexMap<u8, u8, S, 4> = empty!();

    assert!(v.is_empty());
    assert!(s.is_empty());
    assert!(d.is_empty());
    assert!(bh.is_empty());
    assert!(lm.is_empty());
    assert!(is.is_empty());
    assert!(im.is_empty());

    let hv: HeaplessVec<u8, 4> = empty!();
    let hs: HeaplessString<8> = empty!();
    let hd: HeaplessDeque<u8, 4> = empty!();
    let hbh: HeaplessHeap<u8, Max, 4> = empty!();
    let hlm: HeaplessLinearMap<u8, u8, 4> = empty!();
    let his: HeaplessIndexSet<u8, S, 4> = empty!();
    let him: HeaplessIndexMap<u8, u8, S, 4> = empty!();

    assert!(hv.is_empty());
    assert!(hs.is_empty());
    assert!(hd.is_empty());
    assert!(hbh.is_empty());
    assert!(hlm.is_empty());
    assert!(his.is_empty());
    assert!(him.is_empty());
}

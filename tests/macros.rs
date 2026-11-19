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
    let absent: Option<u8> = none!();
    let zeroed: u32 = zero!();
    let cleared: [u8; 0] = empty!();
    let fallback: u16 = amplify::default!();

    assert_eq!(absent, None);
    assert_eq!(zeroed, 0);
    assert!(cleared.is_empty());
    assert_eq!(fallback, 0);
}

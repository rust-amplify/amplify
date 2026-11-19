#![cfg(feature = "alloc")]

#[macro_use]
extern crate amplify;

#[test]
fn confined_constructors_are_available_by_path_and_macro_use() {
    let by_path = amplify::tiny_vec![1u8, 2];
    let by_macro_use = tiny_vec![3u8, 4];

    assert_eq!(by_path.as_slice(), &[1, 2]);
    assert_eq!(by_macro_use.as_slice(), &[3, 4]);
}

#[test]
fn local_literal_macros_expand_in_downstream_crates() {
    let owned = s!("Amplify");
    let absent: Option<u8> = none!();

    assert_eq!(owned, "Amplify");
    assert_eq!(absent, None);
}

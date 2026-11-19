#[cfg(all(feature = "alloc", feature = "hex"))]
#[test]
fn confined_blob_uses_the_reexported_hex_trait() {
    use amplify::hex::FromHex;

    let blob: amplify::confined::TinyBlob = amplify::confined::TinyBlob::from_hex("0102").unwrap();
    assert_eq!(blob.as_slice(), &[1, 2]);
}

#[cfg(feature = "alloc")]
#[test]
fn confined_lengths_use_the_reexported_numeric_types() {
    let values = amplify::tiny_vec![1u8, 2, 3];
    let len: amplify::num::u24 = values.len_u24();
    assert_eq!(len, 3u16.into());
}

#[cfg(all(feature = "alloc", feature = "serde"))]
#[test]
fn serde_is_forwarded_to_confined() {
    fn assert_serialize<T: serde::Serialize>() {}

    assert_serialize::<amplify::confined::TinyVec<u8>>();
}

#[cfg(all(feature = "alloc", feature = "ascii"))]
#[test]
fn ascii_is_forwarded_to_confined() {
    let _: Option<amplify::confined::TinyAscii> = None;
}

#[cfg(all(feature = "alloc", feature = "std", feature = "indexmap"))]
#[test]
fn indexmap_is_forwarded_to_confined() {
    let values: amplify::confined::TinyIndexMap<u8, u8> = amplify::tiny_imap! { 1 => 2 };
    assert_eq!(values.len(), 1);
}

#![cfg(feature = "derive")]

#[derive(amplify::Wrapper, amplify::WrapperMut)]
struct Wrapped(u8);

impl From<u8> for Wrapped {
    fn from(inner: u8) -> Self {
        Self(inner)
    }
}

#[allow(deprecated)]
#[derive(amplify::AsAny)]
struct AnyValue;

#[test]
fn derives_resolve_the_amplify_trait_paths() {
    use amplify::{Wrapper as _, WrapperMut as _};

    let mut wrapped = Wrapped::from_inner(7);
    assert_eq!(*wrapped.as_inner(), 7);
    *wrapped.as_inner_mut() = 8;
    assert_eq!(*wrapped.as_inner(), 8);
}

#[allow(deprecated)]
#[test]
fn as_any_derive_keeps_its_published_path() {
    use amplify::AsAny as _;

    assert!(AnyValue.as_any().is::<AnyValue>());
}

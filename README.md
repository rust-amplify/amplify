<!-- markdownlint-disable MD033 -->
<div align="center">

# `amplify`

**One dependency for the Rust Amplify family.**

[![Build](https://github.com/rust-amplify/rust-amplify/workflows/Build/badge.svg)](https://github.com/rust-amplify/rust-amplify/actions)
[![Tests](https://github.com/rust-amplify/rust-amplify/workflows/Tests/badge.svg)](https://github.com/rust-amplify/rust-amplify/actions)
[![crates.io](https://img.shields.io/crates/v/amplify)](https://crates.io/crates/amplify)
[![Docs](https://docs.rs/amplify/badge.svg)](https://docs.rs/amplify)
[![MIT licensed](https://img.shields.io/badge/license-MIT-blue.svg)](./LICENSE)

</div>

`amplify` is the facade and shared utility crate for the Rust Amplify family. It
re-exports the family crates and keeps a small set of wrappers, array types,
derive targets, and utilities used across them. It is not a zero-dependency
crate: the facade depends on `amplify_num` and `confined`.

- Rust edition: 2021
- Minimum supported Rust version: 1.100
- `no_std` is supported with `--no-default-features`; `alloc` and `std` are
  independently selectable features.

## The Amplify family

| Crate | Purpose |
| --- | --- |
| [`amplify_num`](https://crates.io/crates/amplify_num) | Bit-sized integers, big integers, and optional hexadecimal conversions |
| [`confined`](https://crates.io/crates/confined) | Bounded collections and collection traits |
| `amplify_derive` | Derives for wrappers, formatting, errors, and conversions |
| `amplify_syn` | Helpers for procedural-macro authors |
| `amplify` | Family facade and shared utilities |

The facade re-exports `amplify_num` as `amplify::num`, its hexadecimal module
as `amplify::hex`, and `confined` as `amplify::confined`. Family types therefore
use the same underlying crate versions as the standalone dependencies.

```rust
use amplify::{confined, num};
use amplify::hex::FromHex;

let values = amplify::tiny_vec![1u8, 2, 3];
let length: num::u24 = values.len_u24();
assert_eq!(length, 3u16.into());

let bytes: confined::TinyBlob = confined::TinyBlob::from_hex("0102").unwrap();
assert_eq!(bytes.as_slice(), &[1, 2]);
```

## Modules and compatibility paths

| Path | Contents |
| --- | --- |
| `amplify::array` | `Array`, `Bytes*`, and `ByteArray`; array type aliases remain re-exported at the crate root |
| `amplify::confined` | Re-export of bounded collection types and traits |
| `amplify::error` | `MultiError` and related conversions |
| `amplify::flags` | `FlagVec` and flag helpers |
| `amplify::io` | I/O adapters and deque traits; standard I/O adapters require `std` |
| `amplify::strategy` | Generic implementation strategy helper `Holder` |
| `amplify::wrapper` | `Wrapper` and `WrapperMut` traits, also re-exported at the root |
| `amplify::ffi` | Raw C string conversions, available with `c_raw` except on `wasm32` |

Derive output relies on the stable paths `amplify::Wrapper`,
`amplify::WrapperMut`, `amplify::AsAny`, and (when enabled) `amplify::hex`.
`AsAny` remains available for compatibility but is deprecated in favor of
`core::any::Any` and trait upcasting.

## Features

Default features are `std`, `derive`, and `hex`.

| Feature | Enables |
| --- | --- |
| `alloc` | Allocation-backed family collections and constructor macros |
| `std` | `alloc`, standard-library integrations, and `std` support in family crates |
| `derive` | Re-export of `amplify_derive` macros |
| `proc_attr` | Re-export of `amplify_syn` helpers |
| `hex` | Hexadecimal support in `amplify_num` and `confined` |
| `serde` | Serde support forwarded to family crates |
| `ascii` / `ascii-serde` | Optional confined ASCII types and their Serde support |
| `indexmap`, `hashbrown`, `heapless` | Optional confined collection backends |
| `rand` | Random byte-array generation |
| `stringly_conversions` | Re-export of the optional `stringly_conversions` crate |
| `c_raw` | Standard-library C string conversion helpers in `amplify::ffi` |

`serde` does not implicitly enable `std`; enable `alloc` when using Serde with
allocation-backed values. `rand` users targeting WebAssembly must select the
appropriate `getrandom` backend in their application.

## 4.x to 5.0 migration

5.0 intentionally changes APIs while adopting the new `amplify_num` and
`confined` releases. Several utility types are now module-only:

| 4.x | 5.0 |
| --- | --- |
| `amplify::IoError`, `WriteCounter`, `ConfinedIo`, `CursorDeque` | `amplify::io::{IoError, WriteCounter, ConfinedIo, CursorDeque}` |
| `amplify::MultiError`, `IntoMultiError` | `amplify::error::{MultiError, IntoMultiError}` |
| `amplify::FlagVec` | `amplify::flags::FlagVec` |
| Raw C string helpers at the root | `amplify::ffi::*` with `c_raw` enabled |
| `RawArray` and `from_slice_unsafe` | Removed; use `ByteArray` or checked slice constructors |
| Generic `Array::from_slice` returning `Option` | Use `Array::copy_from_slice`; byte arrays now have `Array::from_slice` returning `Result` |
| `amplify::confinement` | Use `amplify::confined` |

The `smallvec` feature has been removed. Use the collection backends provided
by `confined` (`indexmap`, `hashbrown`, or `heapless`) where appropriate.

## Build and test

```sh
cargo test --all-features
cargo check --all-targets --no-default-features
```

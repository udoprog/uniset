# uniset

[<img alt="github" src="https://img.shields.io/badge/github-udoprog/uniset-8da0cb?style=for-the-badge&logo=github" height="20">](https://github.com/udoprog/uniset)
[<img alt="crates.io" src="https://img.shields.io/crates/v/uniset.svg?style=for-the-badge&color=fc8d62&logo=rust" height="20">](https://crates.io/crates/uniset)
[<img alt="docs.rs" src="https://img.shields.io/badge/docs.rs-uniset-66c2a5?style=for-the-badge&logoColor=white&logo=data:image/svg+xml;base64,PHN2ZyByb2xlPSJpbWciIHhtbG5zPSJodHRwOi8vd3d3LnczLm9yZy8yMDAwL3N2ZyIgdmlld0JveD0iMCAwIDUxMiA1MTIiPjxwYXRoIGZpbGw9IiNmNWY1ZjUiIGQ9Ik00ODguNiAyNTAuMkwzOTIgMjE0VjEwNS41YzAtMTUtOS4zLTI4LjQtMjMuNC0zMy43bC0xMDAtMzcuNWMtOC4xLTMuMS0xNy4xLTMuMS0yNS4zIDBsLTEwMCAzNy41Yy0xNC4xIDUuMy0yMy40IDE4LjctMjMuNCAzMy43VjIxNGwtOTYuNiAzNi4yQzkuMyAyNTUuNSAwIDI2OC45IDAgMjgzLjlWMzk0YzAgMTMuNiA3LjcgMjYuMSAxOS45IDMyLjJsMTAwIDUwYzEwLjEgNS4xIDIyLjEgNS4xIDMyLjIgMGwxMDMuOS01MiAxMDMuOSA1MmMxMC4xIDUuMSAyMi4xIDUuMSAzMi4yIDBsMTAwLTUwYzEyLjItNi4xIDE5LjktMTguNiAxOS45LTMyLjJWMjgzLjljMC0xNS05LjMtMjguNC0yMy40LTMzLjd6TTM1OCAyMTQuOGwtODUgMzEuOXYtNjguMmw4NS0zN3Y3My4zek0xNTQgMTA0LjFsMTAyLTM4LjIgMTAyIDM4LjJ2LjZsLTEwMiA0MS40LTEwMi00MS40di0uNnptODQgMjkxLjFsLTg1IDQyLjV2LTc5LjFsODUtMzguOHY3NS40em0wLTExMmwtMTAyIDQxLjQtMTAyLTQxLjR2LS42bDEwMi0zOC4yIDEwMiAzOC4ydi42em0yNDAgMTEybC04NSA0Mi41di03OS4xbDg1LTM4Ljh2NzUuNHptMC0xMTJsLTEwMiA0MS40LTEwMi00MS40di0uNmwxMDItMzguMiAxMDIgMzguMnYuNnoiPjwvcGF0aD48L3N2Zz4K" height="20">](https://docs.rs/uniset)
[<img alt="build status" src="https://img.shields.io/github/actions/workflow/status/udoprog/uniset/ci.yml?branch=main&style=for-the-badge" height="20">](https://github.com/udoprog/uniset/actions?query=branch%3Amain)

A hierarchical, growable bit set with support for in-place atomic
operations.

The idea is based on [hibitset], but dynamically growing instead of having a
fixed capacity. By being careful with the underlying data layout, we also
support structural sharing between the [local] and [atomic] bitsets.

<br>

## Features

* `vec-safety` - Avoid relying on the assumption that `&mut Vec<T>` can be
  safely coerced to `&mut Vec<U>` if `T` and `U` have an identical memory
  layouts (enabled by default, [issue #1]).

<br>

## Examples

```rust
use uniset::BitSet;

let mut set = BitSet::new();
assert!(set.is_empty());
assert_eq!(0, set.capacity());

set.set(127);
set.set(128);
assert!(!set.is_empty());

assert!(set.test(128));
assert_eq!(vec![127, 128], set.iter().collect::<Vec<_>>());
assert!(!set.is_empty());

assert_eq!(vec![127, 128], set.drain().collect::<Vec<_>>());
assert!(set.is_empty());
```

[issue #1]: https://github.com/udoprog/unicycle/issues/1
[hibitset]: https://docs.rs/hibitset
[local]: https://docs.rs/uniset/latest/uniset/struct.BitSet.html
[atomic]: https://docs.rs/uniset/latest/uniset/struct.AtomicBitSet.html

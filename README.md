# uniset

[![Documentation](https://docs.rs/uniset/badge.svg)](https://docs.rs/uniset)
[![Crates](https://img.shields.io/crates/v/uniset.svg)](https://crates.io/crates/uniset)
[![Actions Status](https://github.com/udoprog/uniset/workflows/Rust/badge.svg)](https://github.com/udoprog/uniset/actions)

A hierarchical, growable bit set with support for in-place atomic operations.

The idea is based on [hibitset], but dynamically growing instead of using a
fixed capacity.
By being careful with the data layout, we can also support structural
sharing between the local and atomic bitset variants.

[hibitset]: https://docs.rs/hibitset

## Features

* `vec-safety` - Avoid relying on the assumption that `&mut Vec<T>` can be
  safely coerced to `&mut Vec<U>` if `T` and `U` have an identical memory
  layouts (enabled by default, [issue #1]).

[issue #1]: https://github.com/udoprog/unicycle/issues/1

# Examples

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
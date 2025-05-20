//! [<img alt="github" src="https://img.shields.io/badge/github-udoprog/uniset-8da0cb?style=for-the-badge&logo=github" height="20">](https://github.com/udoprog/uniset)
//! [<img alt="crates.io" src="https://img.shields.io/crates/v/uniset.svg?style=for-the-badge&color=fc8d62&logo=rust" height="20">](https://crates.io/crates/uniset)
//! [<img alt="docs.rs" src="https://img.shields.io/badge/docs.rs-uniset-66c2a5?style=for-the-badge&logoColor=white&logo=data:image/svg+xml;base64,PHN2ZyByb2xlPSJpbWciIHhtbG5zPSJodHRwOi8vd3d3LnczLm9yZy8yMDAwL3N2ZyIgdmlld0JveD0iMCAwIDUxMiA1MTIiPjxwYXRoIGZpbGw9IiNmNWY1ZjUiIGQ9Ik00ODguNiAyNTAuMkwzOTIgMjE0VjEwNS41YzAtMTUtOS4zLTI4LjQtMjMuNC0zMy43bC0xMDAtMzcuNWMtOC4xLTMuMS0xNy4xLTMuMS0yNS4zIDBsLTEwMCAzNy41Yy0xNC4xIDUuMy0yMy40IDE4LjctMjMuNCAzMy43VjIxNGwtOTYuNiAzNi4yQzkuMyAyNTUuNSAwIDI2OC45IDAgMjgzLjlWMzk0YzAgMTMuNiA3LjcgMjYuMSAxOS45IDMyLjJsMTAwIDUwYzEwLjEgNS4xIDIyLjEgNS4xIDMyLjIgMGwxMDMuOS01MiAxMDMuOSA1MmMxMC4xIDUuMSAyMi4xIDUuMSAzMi4yIDBsMTAwLTUwYzEyLjItNi4xIDE5LjktMTguNiAxOS45LTMyLjJWMjgzLjljMC0xNS05LjMtMjguNC0yMy40LTMzLjd6TTM1OCAyMTQuOGwtODUgMzEuOXYtNjguMmw4NS0zN3Y3My4zek0xNTQgMTA0LjFsMTAyLTM4LjIgMTAyIDM4LjJ2LjZsLTEwMiA0MS40LTEwMi00MS40di0uNnptODQgMjkxLjFsLTg1IDQyLjV2LTc5LjFsODUtMzguOHY3NS40em0wLTExMmwtMTAyIDQxLjQtMTAyLTQxLjR2LS42bDEwMi0zOC4yIDEwMiAzOC4ydi42em0yNDAgMTEybC04NSA0Mi41di03OS4xbDg1LTM4Ljh2NzUuNHptMC0xMTJsLTEwMiA0MS40LTEwMi00MS40di0uNmwxMDItMzguMiAxMDIgMzguMnYuNnoiPjwvcGF0aD48L3N2Zz4K" height="20">](https://docs.rs/uniset)
//!
//! A hierarchical, growable bit set with support for in-place atomic
//! operations.
//!
//! The idea is based on [hibitset], but dynamically growing instead of having a
//! fixed capacity. By being careful with the underlying data layout, we also
//! support structural sharing between the [local] and [atomic] bitsets.
//!
//! <br>
//!
//! ## Examples
//!
//! ```
//! use uniset::BitSet;
//!
//! let mut set = BitSet::new();
//! assert!(set.is_empty());
//! assert_eq!(0, set.capacity());
//!
//! set.set(127);
//! set.set(128);
//! assert!(!set.is_empty());
//!
//! assert!(set.test(128));
//! assert_eq!(vec![127, 128], set.iter().collect::<Vec<_>>());
//! assert!(!set.is_empty());
//!
//! assert_eq!(vec![127, 128], set.drain().collect::<Vec<_>>());
//! assert!(set.is_empty());
//! ```
//!
//! [issue #1]: https://github.com/udoprog/unicycle/issues/1
//! [hibitset]: https://docs.rs/hibitset
//! [local]: https://docs.rs/uniset/latest/uniset/struct.BitSet.html
//! [atomic]: https://docs.rs/uniset/latest/uniset/struct.AtomicBitSet.html

#![deny(missing_docs)]
#![allow(clippy::identity_op)]
#![no_std]

#[cfg(feature = "alloc")]
extern crate alloc;

#[cfg(not(feature = "alloc"))]
compile_error!("The `alloc` feature is required to use this crate.");

use core::fmt;
use core::iter;
use core::mem::{replace, take, ManuallyDrop};
use core::ops;
use core::slice;
use core::sync::atomic::{AtomicUsize, Ordering};

use alloc::vec::Vec;

use self::layers::Layers;

/// A private marker trait that promises that the implementing type has an
/// identical memory layout to another Layer].
///
/// The only purpose of this trait is to server to make [`convert_layers`]
/// safer.
///
/// # Safety
///
/// Implementer must assert that the implementing type has an identical layout
/// to a [Layer].
unsafe trait CoerceLayer {
    /// The target layer being coerced into.
    type Target;
}

/// Bits in a single usize.
const BITS: usize = usize::BITS as usize;
const BITS_SHIFT: usize = BITS.trailing_zeros() as usize;
const MAX_LAYERS: usize = BITS / 4;

/// Precalculated shifts for each layer.
///
/// The shift is used to shift the bits in a given index to the least
/// significant position so it can be used as an index for that layer.
static SHIFT: [usize; 12] = [
    0,
    1 * BITS_SHIFT,
    2 * BITS_SHIFT,
    3 * BITS_SHIFT,
    4 * BITS_SHIFT,
    5 * BITS_SHIFT,
    6 * BITS_SHIFT,
    7 * BITS_SHIFT,
    8 * BITS_SHIFT,
    9 * BITS_SHIFT,
    10 * BITS_SHIFT,
    11 * BITS_SHIFT,
];

/// Same as `SHIFT`, but shifted to the "layer above it".
static SHIFT2: [usize; 12] = [
    1 * BITS_SHIFT,
    2 * BITS_SHIFT,
    3 * BITS_SHIFT,
    4 * BITS_SHIFT,
    5 * BITS_SHIFT,
    6 * BITS_SHIFT,
    7 * BITS_SHIFT,
    8 * BITS_SHIFT,
    9 * BITS_SHIFT,
    10 * BITS_SHIFT,
    11 * BITS_SHIFT,
    12 * BITS_SHIFT,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct LayerLayout {
    /// The length of the layer.
    cap: usize,
}

/// A sparse, layered bit set.
///
/// Layered bit sets support efficient iteration, union, and intersection
/// operations since they maintain summary layers of the bits which are set in
/// layers below it.
///
/// [`BitSet`] and [`AtomicBitSet`]'s are guaranteed to have an identical memory
/// layout, so they support zero-cost back and forth conversion.
///
/// The [`into_atomic`] and [`as_atomic`] methods are provided for converting to
/// an [`AtomicBitSet`].
///
/// [`into_atomic`]: BitSet::into_atomic
/// [`as_atomic`]: BitSet::as_atomic
#[repr(C)]
#[derive(Clone)]
pub struct BitSet {
    /// Layers of bits.
    layers: Layers<Layer>,
    /// The capacity of the bitset in number of bits it can store.
    cap: usize,
}

impl BitSet {
    /// Construct a new, empty BitSet with an empty capacity.
    ///
    /// # Examples
    ///
    /// ```
    /// use uniset::BitSet;
    ///
    /// let mut set = BitSet::new();
    /// assert!(set.is_empty());
    /// assert_eq!(0, set.capacity());
    /// ```
    pub fn new() -> Self {
        Self {
            layers: Layers::new(),
            cap: 0,
        }
    }

    /// Construct a new, empty [`BitSet`] with the specified capacity.
    ///
    /// # Examples
    ///
    /// ```
    /// use uniset::BitSet;
    ///
    /// let mut set = BitSet::with_capacity(1024);
    /// assert!(set.is_empty());
    /// assert_eq!(1024, set.capacity());
    /// ```
    pub fn with_capacity(capacity: usize) -> Self {
        let mut this = Self::new();
        this.reserve(capacity);
        this
    }

    /// Test if the bit set is empty.
    ///
    /// # Examples
    ///
    /// ```
    /// use uniset::BitSet;
    ///
    /// let mut set = BitSet::with_capacity(64);
    /// assert!(set.is_empty());
    /// set.set(2);
    /// assert!(!set.is_empty());
    /// set.clear(2);
    /// assert!(set.is_empty());
    /// ```
    pub fn is_empty(&self) -> bool {
        // The top, summary layer is zero.
        self.layers.last().map(|l| l[0] == 0).unwrap_or(true)
    }

    /// Get the current capacity of the bitset.
    ///
    /// # Examples
    ///
    /// ```
    /// use uniset::BitSet;
    ///
    /// let mut set = BitSet::new();
    /// assert!(set.is_empty());
    /// assert_eq!(0, set.capacity());
    /// ```
    pub fn capacity(&self) -> usize {
        self.cap
    }

    /// Return a slice of the underlying, raw layers.
    ///
    /// # Examples
    ///
    /// ```
    /// use uniset::BitSet;
    ///
    /// let mut set = BitSet::with_capacity(128);
    /// set.set(1);
    /// set.set(5);
    /// // Note: two layers since we specified a capacity of 128.
    /// assert_eq!(vec![&[0b100010, 0][..], &[1]], set.as_slice());
    /// ```
    pub fn as_slice(&self) -> &[Layer] {
        self.layers.as_slice()
    }

    /// Return a mutable slice of the underlying, raw layers.
    pub fn as_mut_slice(&mut self) -> &mut [Layer] {
        self.layers.as_mut_slice()
    }

    /// Convert in-place into an [`AtomicBitSet`].
    ///
    /// Atomic bit sets uses structural sharing with the current set, so this
    /// is a constant time `O(1)` operation.
    ///
    /// # Examples
    ///
    /// ```
    /// use uniset::BitSet;
    ///
    /// let mut set = BitSet::with_capacity(1024);
    ///
    /// let atomic = set.into_atomic();
    /// atomic.set(42);
    ///
    /// let set = atomic.into_local();
    /// assert!(set.test(42));
    /// ```
    pub fn into_atomic(mut self) -> AtomicBitSet {
        AtomicBitSet {
            layers: convert_layers(take(&mut self.layers)),
            cap: replace(&mut self.cap, 0),
        }
    }

    /// Convert in-place into a reference to an [`AtomicBitSet`].
    ///
    /// # Examples
    ///
    /// ```
    /// use uniset::BitSet;
    ///
    /// let set = BitSet::with_capacity(1024);
    ///
    /// set.as_atomic().set(42);
    /// assert!(set.test(42));
    /// ```
    pub fn as_atomic(&self) -> &AtomicBitSet {
        // Safety: BitSet and AtomicBitSet are guaranteed to have identical
        // memory layouts.
        unsafe { &*(self as *const _ as *const AtomicBitSet) }
    }

    /// Set the given bit.
    ///
    /// # Examples
    ///
    /// ```
    /// use uniset::BitSet;
    ///
    /// let mut set = BitSet::with_capacity(64);
    ///
    /// assert!(set.is_empty());
    /// set.set(2);
    /// assert!(!set.is_empty());
    /// ```
    pub fn set(&mut self, mut position: usize) {
        if position >= self.cap {
            self.reserve(position + 1);
        }

        for layer in &mut self.layers {
            let slot = position / BITS;
            let offset = position % BITS;
            layer.set(slot, offset);
            position >>= BITS_SHIFT;
        }
    }

    /// Clear the given bit.
    ///
    /// # Panics
    ///
    /// Panics if the position does not fit within the capacity of the [`BitSet`].
    ///
    /// # Examples
    ///
    /// ```
    /// use uniset::BitSet;
    ///
    /// let mut set = BitSet::with_capacity(64);
    ///
    /// set.clear(2);
    /// assert!(set.is_empty());
    /// set.set(2);
    /// assert!(!set.is_empty());
    /// set.clear(2);
    /// assert!(set.is_empty());
    /// set.clear(2);
    /// assert!(set.is_empty());
    /// ```
    pub fn clear(&mut self, mut position: usize) {
        if position >= self.cap {
            return;
        }

        for layer in &mut self.layers {
            let slot = position / BITS;
            let offset = position % BITS;
            layer.clear(slot, offset);
            position >>= BITS_SHIFT;
        }
    }

    /// Test if the given position is set.
    ///
    /// # Examples
    ///
    /// ```
    /// use uniset::BitSet;
    ///
    /// let mut set = BitSet::with_capacity(64);
    ///
    /// assert!(set.is_empty());
    /// set.set(2);
    /// assert!(!set.is_empty());
    /// assert!(set.test(2));
    /// assert!(!set.test(3));
    /// ```
    pub fn test(&self, position: usize) -> bool {
        if position >= self.cap {
            return false;
        }

        let slot = position / BITS;
        let offset = position % BITS;
        self.layers[0].test(slot, offset)
    }

    /// Reserve enough space to store the given number of elements.
    ///
    /// This will not reserve space for exactly as many elements specified, but
    /// will round up to the closest order of magnitude of 2.
    ///
    /// # Examples
    ///
    /// ```
    /// use uniset::BitSet;
    /// let mut set = BitSet::with_capacity(128);
    /// assert_eq!(128, set.capacity());
    /// set.reserve(250);
    /// assert_eq!(256, set.capacity());
    /// ```
    pub fn reserve(&mut self, cap: usize) {
        if self.cap >= cap {
            return;
        }

        let cap = round_capacity_up(cap);
        let mut new = bit_set_layout(cap).peekable();

        let mut old = self.layers.as_mut_slice().iter_mut();

        while let (Some(layer), Some(&LayerLayout { cap, .. })) = (old.next(), new.peek()) {
            debug_assert!(cap >= layer.cap);

            // Layer needs to grow.
            if cap > 0 {
                layer.grow(cap);
            }

            // Skip to next new layer.
            new.next();
        }

        if self.layers.is_empty() {
            self.layers.extend(new.map(|l| Layer::with_capacity(l.cap)));
        } else {
            // Fill in new layers since we needed to expand.
            //
            // Note: structure is guaranteed to only have one usize at the top
            // so we only need to bother looking at that when we grow.
            for (depth, l) in (self.layers.len() - 1..).zip(new) {
                let mut layer = Layer::with_capacity(l.cap);
                layer[0] = if self.layers[depth][0] > 0 { 1 } else { 0 };
                self.layers.push(layer);
            }
        }

        // Add new layers!
        self.cap = cap;
    }

    /// Create a draining iterator over the bitset, yielding all elements in
    /// order of their index.
    ///
    /// # Examples
    ///
    /// ```
    /// use uniset::BitSet;
    ///
    /// let mut set = BitSet::with_capacity(128);
    /// set.set(127);
    /// set.set(32);
    /// set.set(3);
    ///
    /// assert_eq!(vec![3, 32, 127], set.drain().collect::<Vec<_>>());
    /// assert!(set.is_empty());
    /// ```
    ///
    /// Draining one bit at a time.
    ///
    /// ```
    /// use uniset::BitSet;
    ///
    /// let mut set = BitSet::with_capacity(128);
    ///
    /// set.set(127);
    /// set.set(32);
    /// set.set(3);
    ///
    /// assert_eq!(Some(3), set.drain().next());
    /// assert_eq!(Some(32), set.drain().next());
    /// assert_eq!(Some(127), set.drain().next());
    /// assert!(set.is_empty());
    /// ```
    ///
    /// Saving the state of the draining iterator.
    ///
    /// ```
    /// use uniset::BitSet;
    ///
    /// let mut set = BitSet::with_capacity(128);
    ///
    /// set.set(127);
    /// set.set(32);
    /// set.set(3);
    ///
    /// let mut it = set.drain();
    ///
    /// assert_eq!(Some(3), it.next());
    /// assert_eq!(Some(32), it.next());
    /// assert!(it.snapshot().is_some());
    /// assert_eq!(Some(127), it.next());
    /// assert!(it.snapshot().is_none());
    /// assert_eq!(None, it.next());
    /// assert!(it.snapshot().is_none());
    /// ```
    pub fn drain(&mut self) -> Drain<'_> {
        let depth = self.layers.len().saturating_sub(1);

        Drain {
            layers: self.layers.as_mut_slice(),
            index: 0,
            depth,
            #[cfg(uniset_op_count)]
            op_count: 0,
        }
    }

    /// Start a drain operation using the given configuration parameters.
    ///
    /// These are acquired from [Drain::snapshot], and can be used to resume
    /// draining at a specific point.
    ///
    /// Resuming a drain from a snapshot can be more efficient in certain
    /// scenarios, like if the [`BitSet`] is very large.
    ///
    /// # Examples
    ///
    /// ```
    /// use uniset::BitSet;
    ///
    /// let mut set = BitSet::with_capacity(128);
    ///
    /// set.set(127);
    /// set.set(32);
    /// set.set(3);
    ///
    /// let mut it = set.drain();
    ///
    /// assert_eq!(Some(3), it.next());
    /// let snapshot = it.snapshot();
    /// // Get rid of the existing iterator.
    /// drop(it);
    ///
    /// let snapshot = snapshot.expect("draining iteration hasn't ended");
    ///
    /// let mut it = set.drain_from(snapshot);
    /// assert_eq!(Some(32), it.next());
    /// assert_eq!(Some(127), it.next());
    /// assert_eq!(None, it.next());
    /// ```
    ///
    /// Trying to snapshot from an empty draining iterator:
    ///
    /// ```
    /// use uniset::BitSet;
    ///
    /// let mut set = BitSet::with_capacity(128);
    ///
    /// set.set(3);
    ///
    /// let mut it = set.drain();
    ///
    /// assert!(it.snapshot().is_some());
    /// assert_eq!(Some(3), it.next());
    /// assert!(it.snapshot().is_none());
    /// ```
    pub fn drain_from(&mut self, DrainSnapshot(index, depth): DrainSnapshot) -> Drain<'_> {
        Drain {
            layers: self.layers.as_mut_slice(),
            index,
            depth,
            #[cfg(uniset_op_count)]
            op_count: 0,
        }
    }

    /// Create an iterator over the bitset, yielding all elements in order of
    /// their index.
    ///
    /// Note that iterator allocates a vector with a size matching the number of
    /// layers in the [`BitSet`].
    ///
    /// # Examples
    ///
    /// ```
    /// use uniset::BitSet;
    ///
    /// let mut set = BitSet::with_capacity(128);
    /// set.set(127);
    /// set.set(32);
    /// set.set(3);
    ///
    /// assert_eq!(vec![3, 32, 127], set.iter().collect::<Vec<_>>());
    /// assert!(!set.is_empty());
    /// ```
    pub fn iter(&self) -> Iter<'_> {
        let depth = self.layers.len().saturating_sub(1);

        Iter {
            layers: self.layers.as_slice(),
            masks: [0; MAX_LAYERS],
            index: 0,
            depth,
            #[cfg(uniset_op_count)]
            op_count: 0,
        }
    }
}

impl<I: slice::SliceIndex<[Layer]>> ops::Index<I> for BitSet {
    type Output = I::Output;

    #[inline]
    fn index(&self, index: I) -> &Self::Output {
        ops::Index::index(self.as_slice(), index)
    }
}

impl<I: slice::SliceIndex<[Layer]>> ops::IndexMut<I> for BitSet {
    #[inline]
    fn index_mut(&mut self, index: I) -> &mut Self::Output {
        ops::IndexMut::index_mut(self.as_mut_slice(), index)
    }
}

impl Default for BitSet {
    fn default() -> Self {
        Self::new()
    }
}

/// The snapshot of a drain in progress. This is created using
/// [Drain::snapshot].
///
/// See [BitSet::drain_from] for examples.
#[derive(Clone, Copy)]
pub struct DrainSnapshot(usize, usize);

/// A draining iterator of a [`BitSet`].
///
/// See [BitSet::drain] for examples.
pub struct Drain<'a> {
    layers: &'a mut [Layer],
    index: usize,
    depth: usize,
    #[cfg(uniset_op_count)]
    pub(crate) op_count: usize,
}

impl Drain<'_> {
    /// Save a snapshot of the of the draining iterator, unless it is done
    /// already. This can then be used by [BitSet::drain_from] to efficiently
    /// resume iteration from the given snapshot.
    ///
    /// See [BitSet::drain_from] for examples.
    pub fn snapshot(&self) -> Option<DrainSnapshot> {
        if self.layers.is_empty() {
            None
        } else {
            Some(DrainSnapshot(self.index, self.depth))
        }
    }
}

impl Iterator for Drain<'_> {
    type Item = usize;

    fn next(&mut self) -> Option<Self::Item> {
        if self.layers.is_empty() {
            return None;
        }

        loop {
            #[cfg(uniset_op_count)]
            {
                self.op_count += 1;
            }

            let offset = self.index >> SHIFT2[self.depth];
            // Unsafe version:
            // let slot = unsafe { self.layers.get_unchecked_mut(self.depth).get_unchecked_mut(offset) };
            let slot = &mut self.layers[self.depth][offset];

            if *slot == 0 {
                self.layers = &mut [];
                return None;
            }

            if self.depth > 0 {
                // Advance into a deeper layer. We set the base index to
                // the value of the deeper layer.
                //
                // We calculate the index based on the offset that we are
                // currently at and the information we get at the current
                // layer of bits.
                self.index = (offset << SHIFT2[self.depth])
                    + ((slot.trailing_zeros() as usize) << SHIFT[self.depth]);
                self.depth -= 1;
                continue;
            }

            // We are now in layer 0. The number of trailing zeros indicates
            // the bit set.
            let trail = slot.trailing_zeros() as usize;

            // NB: if this doesn't hold, a prior layer lied and we ended up
            // here in vain.
            debug_assert!(trail < BITS);

            let index = self.index + trail;

            // NB: assert that we are actually unsetting a bit.
            debug_assert!(*slot & !(1 << trail) != *slot);

            // Clear the current slot.
            *slot &= !(1 << trail);

            // Slot is not empty yet.
            if *slot != 0 {
                return Some(index);
            }

            // Clear upper layers until we find one that is not set again -
            // then use that as hour new depth.
            for (depth, layer) in (1..).zip(self.layers[1..].iter_mut()) {
                let offset = index >> SHIFT2[depth];
                // Unsafe version:
                // let slot = unsafe { layer.get_unchecked_mut(offset) };
                let slot = &mut layer[offset];

                // If this doesn't hold, then we have previously failed at
                // populating the summary layers of the set.
                debug_assert!(*slot != 0);

                *slot &= !(1 << ((index >> SHIFT[depth]) % BITS));

                if *slot != 0 {
                    // update the index to be the bottom of the next value set
                    // layer.
                    self.depth = depth;

                    // We calculate the index based on the offset that we are
                    // currently at and the information we get at the current
                    // layer of bits.
                    self.index = (offset << SHIFT2[depth])
                        + ((slot.trailing_zeros() as usize) << SHIFT[depth]);
                    return Some(index);
                }
            }

            // The entire bitset is cleared. We are done.
            self.layers = &mut [];
            return Some(index);
        }
    }
}

/// An iterator over a [`BitSet`].
///
/// See [BitSet::iter] for examples.
pub struct Iter<'a> {
    layers: &'a [Layer],
    masks: [u8; MAX_LAYERS],
    index: usize,
    depth: usize,
    #[cfg(uniset_op_count)]
    pub(crate) op_count: usize,
}

impl Iterator for Iter<'_> {
    type Item = usize;

    fn next(&mut self) -> Option<Self::Item> {
        if self.layers.is_empty() {
            return None;
        }

        loop {
            #[cfg(uniset_op_count)]
            {
                self.op_count += 1;
            }

            let mask = self.masks[self.depth];

            if mask != BITS as u8 {
                let offset = self.index >> SHIFT2[self.depth];
                // Unsafe version:
                // let slot = unsafe { self.layers.get_unchecked(self.depth).get_unchecked(offset) };
                let slot = self.layers[self.depth][offset];
                let slot = (slot >> mask) << mask;

                if slot != 0 {
                    let tail = slot.trailing_zeros() as usize;
                    self.masks[self.depth] = (tail + 1) as u8;

                    // Advance one layer down, setting the index to the bit matching
                    // the offset we are interested in.
                    if self.depth > 0 {
                        self.index = (offset << SHIFT2[self.depth]) + (tail << SHIFT[self.depth]);
                        self.depth -= 1;
                        continue;
                    }

                    return Some(self.index + tail);
                }
            }

            self.masks[self.depth] = 0;
            self.depth += 1;

            if self.depth == self.layers.len() {
                self.layers = &[];
                return None;
            }
        }
    }
}

/// The same as [`BitSet`], except it provides atomic methods.
///
/// [`BitSet`] and [`AtomicBitSet`]'s are guaranteed to have an identical memory
/// layout, so they support zero-cost back and forth conversion.
///
/// The [`as_local_mut`] and [`into_local`] methods can be used to convert to a
/// local unsynchronized bitset.
///
/// [`as_local_mut`]: AtomicBitSet::as_local_mut
/// [`into_local`]: AtomicBitSet::into_local
#[repr(C)]
pub struct AtomicBitSet {
    /// Layers of bits.
    layers: Layers<AtomicLayer>,
    /// The capacity of the bit set in number of bits it can store.
    cap: usize,
}

impl AtomicBitSet {
    /// Construct a new, empty atomic bit set.
    ///
    /// # Examples
    ///
    /// ```
    /// use uniset::AtomicBitSet;
    ///
    /// let set = AtomicBitSet::new();
    /// let set = set.into_local();
    /// assert!(set.is_empty());
    /// ```
    pub fn new() -> Self {
        Self {
            layers: Layers::new(),
            cap: 0,
        }
    }

    /// Get the current capacity of the bitset.
    ///
    /// # Examples
    ///
    /// ```
    /// use uniset::AtomicBitSet;
    ///
    /// let set = AtomicBitSet::new();
    /// assert_eq!(0, set.capacity());
    /// ```
    pub fn capacity(&self) -> usize {
        self.cap
    }

    /// Set the given bit atomically.
    ///
    /// We can do this to an [`AtomicBitSet`] since the required modifications
    /// that needs to be performed against each layer are idempotent of the
    /// order in which they are applied.
    ///
    /// # Panics
    ///
    /// Call will panic if the position is not within the capacity of the
    /// [`AtomicBitSet`].
    ///
    /// # Examples
    ///
    /// ```
    /// use uniset::BitSet;
    ///
    /// let set = BitSet::with_capacity(1024).into_atomic();
    /// set.set(1000);
    /// let set = set.into_local();
    /// assert!(set.test(1000));
    /// ```
    pub fn set(&self, mut position: usize) {
        assert!(
            position < self.cap,
            "position {} is out of bounds for layer capacity {}",
            position,
            self.cap
        );

        for layer in &self.layers {
            let slot = position / BITS;
            let offset = position % BITS;
            layer.set(slot, offset);
            position >>= BITS_SHIFT;
        }
    }

    /// Convert in-place into a a [`BitSet`].
    ///
    /// This is safe, since this function requires exclusive owned access to the
    /// [`AtomicBitSet`], and we assert that their memory layouts are identical.
    ///
    /// [`BitSet`]: BitSet
    ///
    /// # Examples
    ///
    /// ```
    /// use uniset::BitSet;
    ///
    /// let mut set = BitSet::new();
    /// set.reserve(1024);
    ///
    /// let atomic = set.into_atomic();
    /// atomic.set(42);
    ///
    /// let set = atomic.into_local();
    /// assert!(set.test(42));
    /// ```
    pub fn into_local(mut self) -> BitSet {
        BitSet {
            layers: convert_layers(take(&mut self.layers)),
            cap: replace(&mut self.cap, 0),
        }
    }

    /// Convert in-place into a mutable reference to a [`BitSet`].
    ///
    /// This is safe, since this function requires exclusive mutable access to
    /// the [`AtomicBitSet`], and we assert that their memory layouts are
    /// identical.
    ///
    /// [`BitSet`]: BitSet
    ///
    /// # Examples
    ///
    /// ```
    /// use uniset::BitSet;
    ///
    /// let mut set = BitSet::with_capacity(1024).into_atomic();
    ///
    /// set.set(21);
    /// set.set(42);
    ///
    /// {
    ///     let set = set.as_local_mut();
    ///     // Clearing is only safe with BitSet's since we require exclusive
    ///     // mutable access to the collection being cleared.
    ///     set.clear(21);
    /// }
    ///
    /// let set = set.into_local();
    /// assert!(!set.test(21));
    /// assert!(set.test(42));
    /// ```
    pub fn as_local_mut(&mut self) -> &mut BitSet {
        // Safety: BitSet and AtomicBitSet are guaranteed to have identical
        // internal structures.
        unsafe { &mut *(self as *mut _ as *mut BitSet) }
    }
}

impl Default for AtomicBitSet {
    fn default() -> Self {
        Self::new()
    }
}

/// A single layer of bits.
///
/// This is carefully constructed to be structurally equivalent to
/// [AtomicLayer].
/// So that coercing between the two is sound.
#[repr(C)]
pub struct Layer {
    /// Bits.
    bits: *mut usize,
    cap: usize,
}

unsafe impl CoerceLayer for Layer {
    type Target = AtomicLayer;
}
unsafe impl Send for Layer {}
unsafe impl Sync for Layer {}

impl Layer {
    /// Allocate a new raw layer with the specified capacity.
    ///
    /// # Examples
    ///
    /// ```
    /// use uniset::Layer;
    ///
    /// assert_eq!(vec![0usize; 4], Layer::with_capacity(4));
    /// ```
    pub fn with_capacity(cap: usize) -> Layer {
        // Create an already initialized layer of bits.
        let mut vec = ManuallyDrop::new(Vec::<usize>::with_capacity(cap));

        // SAFETY: We just allocated the vector to fit `cap` number of elements.
        unsafe {
            vec.as_mut_ptr().write_bytes(0, cap);
        }

        Layer {
            bits: vec.as_mut_ptr(),
            cap,
        }
    }

    /// Create an iterator over the raw underlying data for the layer.
    ///
    /// # Examples
    ///
    /// ```
    /// use uniset::Layer;
    ///
    /// let mut layer = Layer::with_capacity(2);
    ///
    /// let mut it = layer.iter();
    /// assert_eq!(Some(&0), it.next());
    /// assert_eq!(Some(&0), it.next());
    /// assert_eq!(None, it.next());
    ///
    /// layer.set(0, 63);
    ///
    /// let mut it = layer.iter();
    /// assert_eq!(Some(&(1 << 63)), it.next());
    /// assert_eq!(Some(&0), it.next());
    /// assert_eq!(None, it.next());
    /// ```
    pub fn iter(&self) -> slice::Iter<'_, usize> {
        self.as_slice().iter()
    }

    /// Return the given layer as a slice.
    ///
    /// # Examples
    ///
    /// ```
    /// use uniset::Layer;
    ///
    /// let mut layer = Layer::with_capacity(2);
    /// assert_eq!(vec![0, 0], layer);
    /// assert_eq!(0, layer.as_slice()[0]);
    /// layer.set(0, 42);
    /// assert_eq!(1 << 42, layer.as_slice()[0]);
    /// ```
    pub fn as_slice(&self) -> &[usize] {
        unsafe { slice::from_raw_parts(self.bits, self.cap) }
    }

    /// Return the given layer as a mutable slice.
    ///
    /// # Examples
    ///
    /// ```
    /// use uniset::Layer;
    ///
    /// let mut layer = Layer::with_capacity(2);
    /// assert_eq!(vec![0, 0], layer);
    /// layer.as_mut_slice()[0] = 42;
    /// assert_eq!(vec![42, 0], layer);
    /// ```
    pub fn as_mut_slice(&mut self) -> &mut [usize] {
        unsafe { slice::from_raw_parts_mut(self.bits, self.cap) }
    }

    /// Reserve exactly the specified number of elements in this layer.
    ///
    /// Each added element is zerod as it is grown.
    ///
    /// # Examples
    ///
    /// ```
    /// use uniset::Layer;
    ///
    /// let mut layer = Layer::with_capacity(0);
    /// assert_eq!(vec![], layer);
    /// layer.grow(2);
    /// assert_eq!(vec![0, 0], layer);
    /// ```
    pub fn grow(&mut self, new: usize) {
        let cap = self.cap;

        // Nothing to do.
        if cap >= new {
            return;
        }

        self.with_mut_vec(|vec| {
            vec.reserve_exact(new - cap);

            // SAFETY: We've reserved sufficient space for the grown layer just
            // above.
            unsafe {
                vec.as_mut_ptr().add(cap).write_bytes(0, new - cap);
                vec.set_len(new);
            }

            debug_assert_eq!(vec.len(), vec.capacity());
        });
    }

    /// Set the given bit in this layer.
    ///
    /// # Examples
    ///
    /// ```
    /// use uniset::Layer;
    ///
    /// let mut layer = Layer::with_capacity(2);
    /// layer.set(0, 63);
    /// assert_eq!(vec![1usize << 63, 0usize], layer);
    /// ```
    pub fn set(&mut self, slot: usize, offset: usize) {
        *self.slot_mut(slot) |= 1 << offset;
    }

    /// Clear the given bit in this layer.
    ///
    /// # Examples
    ///
    /// ```
    /// use uniset::Layer;
    ///
    /// let mut layer = Layer::with_capacity(2);
    /// layer.set(0, 63);
    /// assert_eq!(vec![1usize << 63, 0usize], layer);
    /// layer.clear(0, 63);
    /// assert_eq!(vec![0usize, 0usize], layer);
    /// ```
    pub fn clear(&mut self, slot: usize, offset: usize) {
        *self.slot_mut(slot) &= !(1 << offset);
    }

    /// Set the given bit in this layer, where `element` indicates how many
    /// elements are affected per position.
    ///
    /// # Examples
    ///
    /// ```
    /// use uniset::Layer;
    ///
    /// let mut layer = Layer::with_capacity(2);
    /// assert!(!layer.test(0, 63));
    /// layer.set(0, 63);
    /// assert!(layer.test(0, 63));
    /// ```
    pub fn test(&self, slot: usize, offset: usize) -> bool {
        *self.slot(slot) & (1 << offset) > 0
    }

    #[inline(always)]
    fn slot(&self, slot: usize) -> &usize {
        assert!(slot < self.cap);
        // Safety: We check that the slot fits within the capacity.
        unsafe { &*self.bits.add(slot) }
    }

    #[inline(always)]
    fn slot_mut(&mut self, slot: usize) -> &mut usize {
        assert!(slot < self.cap);
        // Safety: We check that the slot fits within the capacity.
        unsafe { &mut *self.bits.add(slot) }
    }

    #[inline(always)]
    #[allow(unused)]
    unsafe fn get_unchecked(&self, slot: usize) -> usize {
        debug_assert!(slot < self.cap);
        *self.bits.add(slot)
    }

    #[inline(always)]
    #[allow(unused)]
    unsafe fn get_unchecked_mut(&mut self, slot: usize) -> &mut usize {
        debug_assert!(slot < self.cap);
        &mut *self.bits.add(slot)
    }

    #[inline(always)]
    fn with_mut_vec<F>(&mut self, f: F)
    where
        F: FnOnce(&mut Vec<usize>),
    {
        struct Restore<'a> {
            layer: &'a mut Layer,
            vec: ManuallyDrop<Vec<usize>>,
        }

        impl Drop for Restore<'_> {
            #[inline]
            fn drop(&mut self) {
                self.layer.bits = self.vec.as_mut_ptr();
                self.layer.cap = self.vec.capacity();
            }
        }

        let vec = ManuallyDrop::new(unsafe { Vec::from_raw_parts(self.bits, self.cap, self.cap) });

        let mut restore = Restore { layer: self, vec };
        f(&mut restore.vec);
    }
}

impl From<Vec<usize>> for Layer {
    fn from(mut value: Vec<usize>) -> Self {
        if value.len() < value.capacity() {
            value.shrink_to_fit();
        }

        let mut value = ManuallyDrop::new(value);

        Self {
            bits: value.as_mut_ptr(),
            cap: value.capacity(),
        }
    }
}

impl Clone for Layer {
    #[inline]
    fn clone(&self) -> Self {
        let mut vec = ManuallyDrop::new(self.as_slice().to_vec());

        Self {
            bits: vec.as_mut_ptr(),
            cap: vec.capacity(),
        }
    }
}

impl fmt::Debug for Layer {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(fmt, "{:?}", self.as_slice())
    }
}

impl<S> PartialEq<S> for Layer
where
    S: AsRef<[usize]>,
{
    fn eq(&self, other: &S) -> bool {
        self.as_slice() == other.as_ref()
    }
}

impl PartialEq<Layer> for &[usize] {
    fn eq(&self, other: &Layer) -> bool {
        *self == other.as_slice()
    }
}

impl PartialEq<Layer> for Vec<usize> {
    fn eq(&self, other: &Layer) -> bool {
        self.as_slice() == other.as_slice()
    }
}

impl Eq for Layer {}

impl AsRef<[usize]> for Layer {
    fn as_ref(&self) -> &[usize] {
        self.as_slice()
    }
}

impl<I: slice::SliceIndex<[usize]>> ops::Index<I> for Layer {
    type Output = I::Output;

    #[inline]
    fn index(&self, index: I) -> &Self::Output {
        ops::Index::index(self.as_slice(), index)
    }
}

impl<I: slice::SliceIndex<[usize]>> ops::IndexMut<I> for Layer {
    #[inline]
    fn index_mut(&mut self, index: I) -> &mut Self::Output {
        ops::IndexMut::index_mut(self.as_mut_slice(), index)
    }
}

impl Drop for Layer {
    #[inline]
    fn drop(&mut self) {
        unsafe {
            drop(Vec::from_raw_parts(self.bits, self.cap, self.cap));
        }
    }
}

/// A single layer of the bitset, that can be atomically updated.
///
/// This is carefully constructed to be structurally equivalent to
/// [Layer].
/// So that coercing between the two is sound.
#[repr(C)]
struct AtomicLayer {
    bits: *mut AtomicUsize,
    cap: usize,
}

unsafe impl CoerceLayer for AtomicLayer {
    type Target = Layer;
}
unsafe impl Send for AtomicLayer {}
unsafe impl Sync for AtomicLayer {}

impl AtomicLayer {
    /// Set the given bit in this layer atomically.
    ///
    /// This allows mutating the layer through a shared reference.
    ///
    /// # Examples
    ///
    /// ```
    /// use uniset::BitSet;
    ///
    /// let set = BitSet::with_capacity(64);
    ///
    /// assert!(set.is_empty());
    /// set.as_atomic().set(2);
    /// assert!(!set.is_empty());
    /// ```
    pub fn set(&self, slot: usize, offset: usize) {
        // Ordering: We rely on external synchronization when testing the layers
        // So total memory ordering does not matter as long as we apply all
        // necessary operations to all layers - which is guaranteed by
        // [AtomicBitSet::set].
        self.slot(slot).fetch_or(1 << offset, Ordering::Relaxed);
    }

    /// Return the given layer as a slice.
    #[inline]
    fn as_slice(&self) -> &[AtomicUsize] {
        unsafe { slice::from_raw_parts(self.bits, self.cap) }
    }

    #[inline(always)]
    fn slot(&self, slot: usize) -> &AtomicUsize {
        assert!(slot < self.cap);
        // Safety: We check that the slot fits within the capacity.
        unsafe { &*self.bits.add(slot) }
    }
}

impl AsRef<[AtomicUsize]> for AtomicLayer {
    #[inline]
    fn as_ref(&self) -> &[AtomicUsize] {
        self.as_slice()
    }
}

impl Drop for AtomicLayer {
    #[inline]
    fn drop(&mut self) {
        // Safety: We keep track of the capacity internally.
        unsafe {
            drop(Vec::from_raw_parts(self.bits, self.cap, self.cap));
        }
    }
}

#[inline]
fn round_bits_up(value: usize) -> usize {
    let m = value % BITS;

    if m == 0 {
        value
    } else {
        value + (BITS - m)
    }
}

/// Helper function to generate the necessary layout of the bit set layers
/// given a desired `capacity`.
#[inline]
fn bit_set_layout(capacity: usize) -> impl Iterator<Item = LayerLayout> + Clone {
    let mut cap = round_bits_up(capacity);

    iter::from_fn(move || {
        if cap == 1 {
            return None;
        }

        cap = round_bits_up(cap) / BITS;

        if cap > 0 {
            Some(LayerLayout { cap })
        } else {
            None
        }
    })
}

/// Round up the capacity to be the closest power of 2.
#[inline]
fn round_capacity_up(cap: usize) -> usize {
    if cap == 0 {
        return 0;
    }

    if cap > 1 << 63 {
        return usize::MAX;
    }

    // Cap is already a power of two.
    let cap = if cap == 1usize << cap.trailing_zeros() {
        cap
    } else {
        1usize << (BITS - cap.leading_zeros() as usize)
    };

    usize::max(16, cap)
}

/// Convert a vector into a different type, assuming the constituent type has
/// an identical layout to the converted type.
#[inline]
fn convert_layers<T, U>(vec: Layers<T>) -> Layers<U>
where
    T: CoerceLayer<Target = U>,
{
    debug_assert_eq!(size_of::<T>(), size_of::<U>());
    debug_assert_eq!(align_of::<T>(), align_of::<U>());

    let mut vec = ManuallyDrop::new(vec);

    // Safety: we guarantee safety by requiring that `T` and `U` implements
    // `IsLayer`.
    unsafe { Layers::from_raw_parts(vec.as_mut_ptr() as *mut U, vec.len(), vec.capacity()) }
}

mod layers {
    use core::iter;
    use core::marker;
    use core::mem::ManuallyDrop;
    use core::ops;
    use core::ptr;
    use core::slice;

    use alloc::vec::Vec;

    /// Storage for layers.
    ///
    /// We use this _instead_ of `Vec<T>` since we want layout guarantees.
    ///
    /// Note: this type is underdocumented since it is internal, and its only
    /// goal is to provide an equivalent compatible API as Vec<T>, so look
    /// there for documentation.
    #[repr(C)]
    pub(super) struct Layers<T> {
        data: *mut T,
        len: usize,
        cap: usize,
        _marker: marker::PhantomData<T>,
    }

    unsafe impl<T> Send for Layers<T> where T: Send {}
    unsafe impl<T> Sync for Layers<T> where T: Sync {}

    impl<T> Layers<T> {
        /// Note: Can't be a constant function :(.
        #[inline]
        pub(super) const fn new() -> Self {
            Self {
                data: ptr::dangling_mut(),
                len: 0,
                cap: 0,
                _marker: marker::PhantomData,
            }
        }

        #[inline]
        pub(super) fn as_mut_ptr(&mut self) -> *mut T {
            self.data
        }

        #[inline]
        pub(super) fn len(&self) -> usize {
            self.len
        }

        #[inline]
        pub(super) fn is_empty(&self) -> bool {
            self.len == 0
        }

        #[inline]
        pub(super) fn capacity(&self) -> usize {
            self.cap
        }

        #[inline]
        pub(super) fn as_mut_slice(&mut self) -> &mut [T] {
            unsafe { slice::from_raw_parts_mut(self.data, self.len) }
        }

        #[inline]
        pub(super) fn as_slice(&self) -> &[T] {
            unsafe { slice::from_raw_parts(self.data as *const T, self.len) }
        }

        #[inline]
        pub(super) fn last(&self) -> Option<&T> {
            self.as_slice().last()
        }

        #[inline]
        pub(super) fn push(&mut self, value: T) {
            self.with_mut_vec(|vec| vec.push(value));
        }

        #[inline]
        pub(super) unsafe fn from_raw_parts(data: *mut T, len: usize, cap: usize) -> Self {
            Self {
                data,
                len,
                cap,
                _marker: marker::PhantomData,
            }
        }

        #[inline(always)]
        fn with_mut_vec<F>(&mut self, f: F)
        where
            F: FnOnce(&mut Vec<T>),
        {
            struct Restore<'a, T> {
                layers: &'a mut Layers<T>,
                vec: ManuallyDrop<Vec<T>>,
            }

            impl<T> Drop for Restore<'_, T> {
                #[inline]
                fn drop(&mut self) {
                    self.layers.data = self.vec.as_mut_ptr();
                    self.layers.len = self.vec.len();
                    self.layers.cap = self.vec.capacity();
                }
            }

            let vec =
                ManuallyDrop::new(unsafe { Vec::from_raw_parts(self.data, self.len, self.cap) });

            let mut restore = Restore { layers: self, vec };
            f(&mut restore.vec);
        }
    }

    impl<T> Default for Layers<T> {
        #[inline]
        fn default() -> Self {
            Self::new()
        }
    }

    impl<T> Clone for Layers<T>
    where
        T: Clone,
    {
        #[inline]
        fn clone(&self) -> Self {
            let mut vec =
                ManuallyDrop::new(unsafe { Vec::from_raw_parts(self.data, self.len, self.cap) })
                    .clone();

            Self {
                data: vec.as_mut_ptr(),
                len: vec.len(),
                cap: vec.capacity(),
                _marker: marker::PhantomData,
            }
        }
    }

    impl<'a, T> IntoIterator for &'a mut Layers<T> {
        type IntoIter = slice::IterMut<'a, T>;
        type Item = &'a mut T;

        #[inline]
        fn into_iter(self) -> Self::IntoIter {
            self.as_mut_slice().iter_mut()
        }
    }

    impl<'a, T> IntoIterator for &'a Layers<T> {
        type IntoIter = slice::Iter<'a, T>;
        type Item = &'a T;

        #[inline]
        fn into_iter(self) -> Self::IntoIter {
            self.as_slice().iter()
        }
    }

    impl<T, I: slice::SliceIndex<[T]>> ops::Index<I> for Layers<T> {
        type Output = I::Output;

        #[inline]
        fn index(&self, index: I) -> &Self::Output {
            ops::Index::index(self.as_slice(), index)
        }
    }

    impl<T, I: slice::SliceIndex<[T]>> ops::IndexMut<I> for Layers<T> {
        #[inline]
        fn index_mut(&mut self, index: I) -> &mut Self::Output {
            ops::IndexMut::index_mut(self.as_mut_slice(), index)
        }
    }

    impl<T> iter::Extend<T> for Layers<T> {
        #[inline]
        fn extend<I: IntoIterator<Item = T>>(&mut self, iter: I) {
            self.with_mut_vec(|vec| vec.extend(iter));
        }
    }

    impl<T> Drop for Layers<T> {
        #[inline]
        fn drop(&mut self) {
            drop(unsafe { Vec::from_raw_parts(self.data, self.len, self.cap) });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{bit_set_layout, AtomicBitSet, BitSet};

    use alloc::vec;
    use alloc::vec::Vec;

    #[test]
    fn assert_send_and_sync() {
        assert_traits(BitSet::new());
        assert_traits(AtomicBitSet::new());

        fn assert_traits<T: Send + Sync>(_: T) {}
    }

    #[test]
    fn test_set_and_test() {
        let mut set = BitSet::new();
        set.reserve(1024);
        set.set(1);
        set.set(64);
        set.set(129);
        set.set(1023);

        assert!(set.test(1));
        assert!(set.test(64));
        assert!(set.test(129));
        assert!(set.test(1023));
        assert!(!set.test(1022));

        let mut layer0 = [0usize; 16];
        layer0[0] = 1 << 1;
        layer0[1] = 1;
        layer0[2] = 1 << 1;
        layer0[15] = 1 << 63;

        let mut layer1 = [0usize; 1];
        layer1[0] = (1 << 15) | (1 << 2) | (1 << 1) | 1;

        assert_eq!(vec![&layer0[..], &layer1[..]], set.as_slice());
    }

    #[test]
    fn test_bit_layout() {
        assert!(bit_set_layout(0).collect::<Vec<_>>().is_empty());
        assert_eq!(
            vec![1],
            bit_set_layout(64).map(|l| l.cap).collect::<Vec<_>>()
        );
        assert_eq!(
            vec![2, 1],
            bit_set_layout(128).map(|l| l.cap).collect::<Vec<_>>()
        );
        assert_eq!(
            vec![64, 1],
            bit_set_layout(4096).map(|l| l.cap).collect::<Vec<_>>()
        );
        assert_eq!(
            vec![65, 2, 1],
            bit_set_layout(4097).map(|l| l.cap).collect::<Vec<_>>()
        );
        assert_eq!(
            vec![2, 1],
            bit_set_layout(65).map(|l| l.cap).collect::<Vec<_>>()
        );
        assert_eq!(
            vec![2, 1],
            bit_set_layout(128).map(|l| l.cap).collect::<Vec<_>>()
        );
        assert_eq!(
            vec![3, 1],
            bit_set_layout(129).map(|l| l.cap).collect::<Vec<_>>()
        );
        assert_eq!(
            vec![65, 2, 1],
            bit_set_layout(4097).map(|l| l.cap).collect::<Vec<_>>()
        );
    }

    // NB: test to run through miri to make sure we reserve layers appropriately.
    #[test]
    fn test_reserve() {
        let mut b = BitSet::new();
        b.reserve(1_000);
        b.reserve(10_000);

        assert_ne!(
            bit_set_layout(1_000).collect::<Vec<_>>(),
            bit_set_layout(10_000).collect::<Vec<_>>()
        );
    }

    macro_rules! drain_test {
        ($cap:expr, $sample:expr, $expected_op_count:expr) => {{
            let mut set = BitSet::new();
            set.reserve($cap);

            let positions: Vec<usize> = $sample;

            for p in positions.iter().copied() {
                set.set(p);
            }

            let mut drain = set.drain();
            assert_eq!(positions, (&mut drain).collect::<Vec<_>>());

            #[cfg(uniset_op_count)]
            {
                let op_count = drain.op_count;
                assert_eq!($expected_op_count, op_count);
            }

            // Assert that all layers are zero.
            assert!(set
                .as_slice()
                .into_iter()
                .all(|l| l.iter().all(|n| *n == 0)));
        }};
    }

    macro_rules! iter_test {
        ($cap:expr, $sample:expr, $expected_op_count:expr) => {{
            let mut set = BitSet::new();
            set.reserve($cap);

            let positions: Vec<usize> = $sample;

            for p in positions.iter().copied() {
                set.set(p);
            }

            let mut iter = set.iter();
            assert_eq!(positions, (&mut iter).collect::<Vec<_>>());

            #[cfg(uniset_op_count)]
            {
                let op_count = iter.op_count;
                assert_eq!($expected_op_count, op_count);
            }
        }};
    }

    #[test]
    fn test_drain() {
        drain_test!(0, vec![], 0);
        drain_test!(1024, vec![], 1);
        drain_test!(64, vec![0], 1);
        drain_test!(64, vec![0, 1], 2);
        drain_test!(64, vec![0, 1, 63], 3);
        drain_test!(128, vec![64], 3);
        drain_test!(128, vec![0, 32, 64], 7);
        drain_test!(4096, vec![0, 32, 64, 3030, 4095], 13);
        drain_test!(
            1_000_000,
            vec![0, 32, 64, 3030, 4095, 50_000, 102110, 203020, 500000, 803020, 900900],
            51
        );
        #[cfg(not(miri))]
        drain_test!(1_000_000, (0..1_000_000).collect::<Vec<usize>>(), 1_031_748);
        #[cfg(not(miri))]
        drain_test!(
            10_000_000,
            vec![0, 32, 64, 3030, 4095, 50_000, 102110, 203020, 500000, 803020, 900900, 9_009_009],
            58
        );
    }

    #[test]
    fn test_iter() {
        iter_test!(0, vec![], 0);
        iter_test!(1024, vec![], 1);
        iter_test!(64, vec![0, 2], 3);
        iter_test!(64, vec![0, 1], 3);
        iter_test!(128, vec![64], 4);
        iter_test!(128, vec![0, 32, 64], 8);
        iter_test!(4096, vec![0, 32, 64, 3030, 4095], 14);
        iter_test!(
            1_000_000,
            vec![0, 32, 64, 3030, 4095, 50_000, 102110, 203020, 500000, 803020, 900900],
            52
        );
        #[cfg(not(miri))]
        iter_test!(
            10_000_000,
            vec![0, 32, 64, 3030, 4095, 50_000, 102110, 203020, 500000, 803020, 900900, 9_009_009],
            59
        );
        #[cfg(not(miri))]
        iter_test!(1_000_000, (0..1_000_000).collect::<Vec<usize>>(), 1_031_749);
    }

    #[test]
    fn test_round_capacity_up() {
        use super::round_capacity_up;
        assert_eq!(0, round_capacity_up(0));
        assert_eq!(16, round_capacity_up(1));
        assert_eq!(32, round_capacity_up(17));
        assert_eq!(32, round_capacity_up(32));
        assert_eq!((usize::MAX >> 1) + 1, round_capacity_up(usize::MAX >> 1));
        assert_eq!(usize::MAX, round_capacity_up((1usize << 63) + 1));
    }

    #[test]
    fn test_grow_one_at_a_time() {
        let mut active = BitSet::new();

        for i in 0..128 {
            active.reserve(i);
        }
    }
}

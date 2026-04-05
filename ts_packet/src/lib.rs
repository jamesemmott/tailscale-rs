//! Types for processing network packets.

#![no_std]
mod basic;
mod buf;
pub mod old;
pub mod raw;

extern crate alloc;

use alloc::collections::BTreeMap;
use core::ops::Range;

pub use basic::BasicBatch;
pub use buf::Buffer;

use crate::raw::RawBatch;

/// Describes the boundaries of a packet within a Buffer.
///
/// |............|-----------------|----------------|------------------|.............|
///  <- offset -> <- pre_padding -> <- packet_len -> <- post_padding ->
#[derive(Clone, Debug)]
pub struct PacketLayout {
    /// Absolute start byte offset in Buffer.
    pub offset: usize,
    /// Currently in-use bytes of the packet.
    pub packet_len: usize,
    /// Unused but reserved bytes in front of packet_len.
    pub pre_padding: usize,
    /// Unused but reserved bytes behind packet_len.
    pub post_padding: usize,
}

impl PacketLayout {
    /// Make a layout starting at absolute byte offset `offset`, with `packet_len` bytes and
    /// no padding.
    pub fn new(offset: usize, packet_len: usize) -> PacketLayout {
        Self::with_padding(offset, packet_len, 0, 0)
    }

    /// Make a layout starting at absolute byte offset `offset`, the given length and
    /// pre/post padding.
    pub fn with_padding(
        offset: usize,
        packet_len: usize,
        pre_padding: usize,
        post_padding: usize,
    ) -> PacketLayout {
        PacketLayout {
            offset,
            packet_len,
            pre_padding,
            post_padding,
        }
    }

    /// Total number of bytes owned by this layout.
    pub fn stride(&self) -> usize {
        self.pre_padding + self.packet_len + self.post_padding
    }

    /// First buffer index beyond this layout.
    pub fn stride_end(&self) -> usize {
        self.offset + self.stride()
    }

    /// Buffer range owned by this layout.
    pub fn stride_range(&self) -> Range<usize> {
        self.offset..self.stride_end()
    }

    /// Start of the packet_len range.
    pub fn start(&self) -> usize {
        self.offset + self.pre_padding
    }

    /// End of the packet_len range.
    pub fn end(&self) -> usize {
        self.start() + self.packet_len
    }

    /// Buffer range containing in-use packet bytes (defined by packet_len).
    pub fn range(&self) -> Range<usize> {
        self.start()..self.end()
    }

    /// Report whether all the layout's bytes are within range.
    pub fn is_within(&self, range: Range<usize>) -> bool {
        self.offset >= range.start && self.stride_end() <= range.end
    }

    /// Change packet_len by delta by changing the amount of post_padding.
    ///
    /// # Panics
    ///
    /// If delta would make packet_len or post_padding negative.
    pub fn resize_back(&mut self, delta: isize) {
        if delta < 0 {
            let num_bytes = (-delta) as usize;
            assert!(num_bytes <= self.packet_len);
            self.packet_len -= num_bytes;
            self.post_padding += num_bytes;
        } else {
            let num_bytes = delta as usize;
            assert!(num_bytes <= self.post_padding);
            self.post_padding -= num_bytes;
            self.packet_len += num_bytes;
        }
    }

    /// Change packet_len by delta by changing the amount of pre_padding.
    ///
    /// # Panics
    ///
    /// If delta would make packet_len or pre_padding negative.
    pub fn resize_front(&mut self, num_bytes: isize) {
        if num_bytes < 0 {
            let num_bytes = (-num_bytes) as usize;
            assert!(num_bytes <= self.packet_len);
            self.packet_len -= num_bytes;
            self.pre_padding += num_bytes;
        } else {
            let num_bytes = num_bytes as usize;
            assert!(num_bytes <= self.pre_padding);
            self.pre_padding -= num_bytes;
            self.packet_len += num_bytes;
        }
    }
}

/// A read-only view of a [`Batch`]'s packet.
#[derive(Clone)]
pub struct Packet<'batch, Metadata: Clone> {
    /// The packet's raw bytes.
    ///
    /// The slice does not include the packet's pre- or post-padding.
    pub data: &'batch [u8],
    /// The packet's metadata.
    pub meta: Metadata,
}

/// A mutable view of a [`Batch`]'s packet.
pub struct PacketMut<'batch, Metadata: Clone> {
    /// The packet's raw bytes.
    ///
    /// The slice does not include the packet's pre- or post-padding.
    pub data: &'batch mut [u8],
    /// The packet's metadata.
    pub meta: Metadata,
    /// The layout of `self.data` in the underlying [`Buffer`]. Having access to the layout
    /// allows the caller to reshape the packet. It's private so that the caller must use
    /// `PacketMut`'s methods, which update `self.data` in addition to mutating the layout.
    layout: &'batch mut PacketLayout,
}

impl<'batch, Metadata: Clone> PacketMut<'batch, Metadata> {
    /// Return the packet's layout in the batch's underlying buffer.
    pub fn layout(&self) -> &PacketLayout {
        self.layout
    }

    /// Change packet_len by delta by changing the amount of post_padding.
    ///
    /// `self.data` is updated to reflect the layout change. The newly visible bytes have
    /// arbitrary values.
    ///
    /// # Panics
    ///
    /// If delta would make packet_len or post_padding negative.
    pub fn resize_back(&mut self, delta: isize) {
        self.layout.resize_back(delta);
        let len = self.layout.packet_len;
        let ptr = self.data.as_mut_ptr();
        unsafe {
            // SAFETY: self.data is a view on the bytes described by self.layout.range(). resize_back
            // changes only the end position of layout.range(), and ensures that the packet has
            // unique ownership of the layout. resize_back does not move the underlying data, so the
            // new self.data starts at the same position, and has an updated length.
            self.data = core::slice::from_raw_parts_mut(ptr, len);
        }
    }

    /// Change packet_len by delta by changing the amount of pre_padding.
    ///
    /// `self.data` is updated to reflect the layout change. The newly visible bytes have
    /// arbitrary values.
    ///
    /// # Panics
    ///
    /// If delta would make packet_len or pre_padding negative.
    pub fn resize_front(&mut self, delta: isize) {
        self.layout.resize_front(delta);
        let len = self.layout.packet_len;
        let mut ptr = self.data.as_mut_ptr();
        unsafe {
            // SAFETY: `self.layout.resize_front()` ensures the requested `delta` remains in-bounds
            // of the buffer range owned by this packet. The new ptr address matches the updated
            // `self.layout.range().start` (earlier in memory for a grow, later for a shrink).
            ptr = ptr.offset(-delta);
            // SAFETY: `self.layout.resize_front()` does not move the underlying data, so the new
            // slice still points to valid owned bytes. The new `ptr` and `len` match
            // `self.layout.range()`, as adjusted by `self.layout.resize_front()` above.
            self.data = core::slice::from_raw_parts_mut(ptr, len);
        }
    }

    /// Append `bytes` to the packet, consuming `bytes.len()` bytes of post-padding.
    ///
    /// # Panics
    ///
    /// If insufficient post-padding is available.
    pub fn append_bytes(&mut self, bytes: &[u8]) {
        let old_len = self.data.len();
        self.resize_back(bytes.len() as isize);
        self.data[old_len..].copy_from_slice(bytes);
    }

    /// Prepend `bytes` to the packet, consuming `bytes.len()` bytes of pre-padding.
    ///
    /// # Panics
    ///
    /// If insufficient pre-padding is available.
    pub fn prepend_bytes(&mut self, bytes: &[u8]) {
        self.resize_front(bytes.len() as isize);
        self.data[..bytes.len()].copy_from_slice(bytes);
    }
}

impl<'batch, Metadata: Clone> From<PacketMut<'batch, Metadata>> for Packet<'batch, Metadata> {
    fn from(packet: PacketMut<'batch, Metadata>) -> Packet<'batch, Metadata> {
        Packet {
            data: packet.data,
            meta: packet.meta,
        }
    }
}

/// A batch of packets, and operations on them.
pub trait Batch: Sized + AsRef<RawBatch> + AsMut<RawBatch> {
    /// The type for per-packet metadata.
    type Metadata: Clone;

    /// Return metadata for the `ith` packet in the batch.
    fn get_metadata(&self, i: usize) -> Option<Self::Metadata>;

    /// Return a clone of `self` that can access no packets.
    ///
    /// Returned batches must be usable in [`Batch::batch_by_mut`].
    fn empty_clone(&self) -> Self;

    /// Split `self` into multiple output batches, controlled by `f`.
    ///
    /// The provided `collection` owns the output batches. `f` receives each packet of `self` in
    /// turn, and for each one can return `None` to discard the packet, or
    /// `Some(batch_from_collection)` to move the packet into that batch.
    ///
    /// Batches returned by `f` must have been created using `self.empty_clone()`, either when
    /// initializing `collection` prior to calling `group_by_mut`, or as needed within `f`.
    ///
    /// This method is a building block primitive for other Batch slicing functions like
    /// [`Batch::group_by_mut`] and [`Batch::retain_mut`]. Prefer to use those simpler functions
    /// when possible.
    ///
    /// # Panics
    ///
    /// If `f` returns a batch that was not obtained with `self.empty_clone()`.
    fn batch_by_mut<F, C>(self, f: F, collection: C) -> C
    where
        F: for<'a> FnMut(usize, PacketMut<'_, Self::Metadata>, &'a mut C) -> Option<&'a mut Self>;

    /// Return the number of packets in the batch.
    fn len(&self) -> usize {
        self.as_ref().len()
    }

    /// Report whether the batch is empty.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Get a reference to the `ith` packet in the batch.
    fn get(&self, i: usize) -> Option<Packet<'_, Self::Metadata>> {
        let meta = self.get_metadata(i)?;
        let data = self.as_ref().get_data(i)?;
        Some(Packet { data, meta })
    }

    /// Get a mutable reference to the `ith` packet in the batch.
    fn get_mut(&mut self, i: usize) -> Option<PacketMut<'_, Self::Metadata>> {
        let meta = self.get_metadata(i)?;
        // SAFETY: the unsafe mutable layout reference is handed to PacketMut, which only allows
        // layout mutations that don't change the range of bytes owned by the layout.
        let (data, layout) = unsafe { self.as_mut().get_data_and_layout_mut(i)? };
        Some(PacketMut { data, layout, meta })
    }

    /// Get a reference to the `ith` packet in the batch.
    ///
    /// # Panics
    ///
    /// If `i >= self.len()`
    fn index(&self, i: usize) -> Packet<'_, Self::Metadata> {
        self.get(i).unwrap()
    }

    /// Get a mutable reference to the `ith` packet in the batch.
    ///
    /// # Panics
    ///
    /// If `i >= self.len()`
    fn index_mut(&mut self, i: usize) -> PacketMut<'_, Self::Metadata> {
        self.get_mut(i).unwrap()
    }

    /// Return an iterator over the packets in the batch.
    fn iter(&self) -> BatchIterator<'_, Self> {
        BatchIterator::new(self)
    }

    /// Split `self` into multiple output batches, controlled by `f`.
    ///
    /// The provided `collection` owns the output batches. `f` receives each packet of `self` in
    /// turn, and for each one can return `None` to discard the packet, or
    /// `Some(batch_from_collection)` to move the packet into that batch.
    ///
    /// Batches returned by `f` must have been created using `self.empty_clone()`, either when
    /// initializing `collection` prior to calling `group_by_mut`, or as needed within `f`.
    ///
    /// This method is a building block primitive for other Batch slicing functions like
    /// [`Batch::group_by_mut`] and [`Batch::retain_mut`]. Prefer to use those simpler functions
    /// when possible.
    ///
    /// # Panics
    ///
    /// If `f` returns a batch that was not obtained with `self.empty_clone()`.
    fn batch_by<F, C>(self, mut f: F, collection: C) -> C
    where
        F: for<'a> FnMut(usize, Packet<'_, Self::Metadata>, &'a mut C) -> Option<&'a mut Self>,
    {
        self.batch_by_mut(|i, packet, out| f(i, packet.into(), out), collection)
    }

    /// Call `f` on every packet in the batch.
    ///
    /// The standard Iterator trait cannot express the lifetime constraints that
    /// RawBatch needs to safely hand out PacketMuts, so this method exists to
    /// avoid the boilerplate of manually iterating and indexing packets.
    ///
    /// `f` receives the packet's index in the batch (the value you would pass to `packet_at_mut`),
    /// and the packet itself.
    fn for_each_mut<F>(&mut self, mut f: F)
    where
        F: FnMut(usize, PacketMut<'_, Self::Metadata>),
    {
        for i in 0..self.len() {
            let packet = self.index_mut(i);
            f(i, packet);
        }
    }

    /// Split `self` into a map of keys mapped to batches of matching packets.
    ///
    /// `f` provides the key for each packet of `self`.
    fn group_by_mut<K: Ord>(
        self,
        mut f: impl FnMut(usize, PacketMut<'_, Self::Metadata>) -> K,
    ) -> BTreeMap<K, Self> {
        let ret = BTreeMap::new();
        let empty = self.empty_clone();
        self.batch_by_mut(
            |i, packet, ret| {
                let k = f(i, packet);
                Some(ret.entry(k).or_insert_with(|| empty.empty_clone()))
            },
            ret,
        )
    }

    /// Split `self` into a map of keys mapped to batches of matching packets.
    ///
    /// `f` provides the key for each packet of `self`.
    fn group_by<K: Ord>(
        self,
        mut f: impl FnMut(usize, Packet<'_, Self::Metadata>) -> K,
    ) -> BTreeMap<K, Self> {
        self.group_by_mut(|i, packet| f(i, packet.into()))
    }

    /// Return a batch containing only packets for which `f` returns true.
    fn retain_mut(self, mut f: impl FnMut(usize, PacketMut<'_, Self::Metadata>) -> bool) -> Self {
        let ret = self.empty_clone();
        self.batch_by_mut(
            |i, packet, out| if f(i, packet) { Some(out) } else { None },
            ret,
        )
    }

    /// Return a batch containing only packets for which `f` returns true.
    fn retain(self, mut f: impl FnMut(usize, Packet<'_, Self::Metadata>) -> bool) -> Self {
        self.retain_mut(|i, packet| f(i, packet.into()))
    }

    /// Split the batch in two at the specified index.
    ///
    /// # Panics
    ///
    /// If `idx > self.len()`.
    fn split_at(self, idx: usize) -> (Self, Self) {
        assert!(idx <= self.len());
        let ret = (self.empty_clone(), self.empty_clone());
        self.batch_by(
            |i, _packet, out| Some(if i < idx { &mut out.0 } else { &mut out.1 }),
            ret,
        )
    }

    /// Split the batch in two.
    ///
    /// Returns a pair of batches: all the packets for which `f` returned `true`, and all the
    /// packets for which it returned `false`.
    fn partition_mut(
        self,
        mut f: impl FnMut(usize, PacketMut<'_, Self::Metadata>) -> bool,
    ) -> (Self, Self) {
        let ret = (self.empty_clone(), self.empty_clone());
        self.batch_by_mut(
            |i, packet, out| {
                if f(i, packet) {
                    Some(&mut out.0)
                } else {
                    Some(&mut out.1)
                }
            },
            ret,
        )
    }

    /// Split the batch in two.
    ///
    /// Returns a pair of batches, all the packets for which `f` returned `true`, and all the
    /// packets for which it returned `false`.
    fn partition(
        self,
        mut f: impl FnMut(usize, Packet<'_, Self::Metadata>) -> bool,
    ) -> (Self, Self) {
        self.partition_mut(|i, packet| f(i, packet.into()))
    }
}

/// Iterator over a batch's packets.
///
/// Due to rust's orphan rule for trait implementations, we can't provide a blanket
/// implementation of IntoIterator for all batch types. Batch types can implement IntoIterator
/// trivially:
///
/// # Example
///
/// ```ignore
/// struct MyPacket<'batch> { ... }
///
/// struct MyBatch { ... }
///
/// impl Batch for MyBatch { ... }
///
/// impl<'batch> IntoIterator for &'batch MyBatch {
///     type Item = MyPacket<'batch>;
///     type IntoIter = BatchIterator<'batch, RawBatch>;
///
///     fn into_iter(self) -> Self::IntoIter {
///         BatchIterator::new(self)
///     }
/// }
/// ```
pub struct BatchIterator<'batch, B: Batch> {
    batch: &'batch B,
    position: usize,
}

impl<'batch, B: Batch> BatchIterator<'batch, B> {
    /// Return an iterator for the given batch.
    pub fn new(batch: &'batch B) -> BatchIterator<'batch, B> {
        BatchIterator { batch, position: 0 }
    }
}

impl<'batch, B: Batch> Iterator for BatchIterator<'batch, B> {
    type Item = Packet<'batch, B::Metadata>;

    fn next(&mut self) -> Option<Self::Item> {
        let result = self.batch.get(self.position)?;
        self.position += 1;
        Some(result)
    }
}

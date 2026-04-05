//! Building blocks for making batches of packets. Don't use this directly unless you're making
//! a new batch type.
//!
//! # Safety
//!
//! The key safety invariant is that a batch has unique access to the parts of the buffer that it
//! references.
//!
//! Safety follows from that - mutating methods require a mutable reference to the batch or a
//! MutPacket (to modify a packet in place).
//!
//! To enforce this invariant, batches cannot be cloned (references to batches are fine). Any
//! method which creates a batch from an existing batch must consume the old batch (i.e., take
//! `self` by value).

use alloc::{sync::Arc, vec::Vec};
use core::ops::Range;

use crate::{Batch, PacketLayout, buf::Buffer};

/// A view on part of a Buffer, presented as individual packets.
pub struct RawBatch {
    /// The backing storage for packets.
    buf: Arc<Buffer>,
    /// The geometry of packets owned by this batch.
    ///
    /// These layouts define regions of self.buf that are owned by this batch.
    ///
    /// SAFETY: layouts must not overlap each other, or any other layout that shares the same
    /// Buffer. Layouts must be ordered by ascending offset.
    layouts: Vec<PacketLayout>,
    /// True if layouts have no gaps between them. Some operations have fast paths for contiguous
    /// batches.
    ///
    /// SAFETY: may be true only if self.layouts has no gaps between layouts. Must be false if
    /// self.layouts is empty. It is always safe for this field to be false.
    contiguous: bool,
}

/// Constructs a RawBatch by layering PacketLayouts over a Buffer.
pub struct RawBatchBuilder {
    buf: Arc<Buffer>,
    buf_range: Range<usize>,
    layouts: Vec<PacketLayout>,
    // Note: unlike RawBatch, self.contiguous is true when self.layouts is empty.
    contiguous: bool,
}

impl RawBatchBuilder {
    /// Build a batch over a newly allocated [`Buffer`].
    pub fn new(buf_size: usize) -> Self {
        RawBatchBuilder {
            buf: Arc::new(Buffer::new(buf_size)),
            buf_range: 0..buf_size,
            layouts: Vec::new(),
            contiguous: true,
        }
    }

    /// Build a batch over an existing [`Buffer`].
    pub fn from_buffer(buf: Buffer) -> Self {
        RawBatchBuilder {
            buf_range: 0..buf.data.len(),
            buf: Arc::new(buf),
            layouts: Vec::new(),
            contiguous: true,
        }
    }

    /// Build a batch over a contiguous portion of a shared [`Buffer`]
    ///
    /// # Safety
    ///
    /// The caller must ensure that the builder has unique ownership of `buf[range]`.
    pub unsafe fn from_shared_buffer(buf: Arc<Buffer>, range: Range<usize>) -> Self {
        RawBatchBuilder {
            buf,
            buf_range: range,
            layouts: Vec::new(),
            contiguous: true,
        }
    }

    /// Return the range of bytes that are still available for registration of new packets.
    pub fn available(&self) -> Range<usize> {
        match self.layouts.last() {
            None => self.buf_range.clone(),
            Some(layout) => layout.stride_end()..self.buf_range.end,
        }
    }

    /// Allocate part of the buffer for use by a packet with the given layout.
    ///
    /// The new layout must be located later in the buffer than previously pushed layouts.
    ///
    /// # Panics
    ///
    /// If the layout's placement is not legal.
    // TODO should provide efficient ways to register lots of layouts.
    pub fn push_layout(&mut self, layout: PacketLayout) {
        assert!(layout.is_within(self.available()));
        // SAFETY: the assertion enforces all three required invariants.
        unsafe {
            self.push_layout_unchecked(layout);
        }
    }

    /// Allocate part of the buffer for use by a packet with the given layout.
    ///
    /// # Safety
    ///
    /// Caller must ensure that:
    /// - The layout is within the bounds of the underlying [`Buffer`].
    /// - The layout starts after all layouts previously pushed into this builder.
    /// - This builder has unique ownership of the part of the [`Buffer`]
    ///   described by the layout.
    ///
    /// A layout always complies with these invariants if it is entirely within self.available().
    // TODO: replace automatic update of self.contiguous with an explicit parameter?
    pub unsafe fn push_layout_unchecked(&mut self, layout: PacketLayout) {
        self.contiguous &= layout.offset == self.available().start;
        self.layouts.push(layout);
    }

    /// Consume the builder and return the built batch.
    pub fn finish(self) -> RawBatch {
        let contiguous = !self.layouts.is_empty() && self.contiguous;
        RawBatch {
            buf: self.buf,
            layouts: self.layouts,
            contiguous,
        }
    }
}

impl RawBatch {
    /// Get the bytes of the `ith` packet in the batch.
    pub fn get_data(&self, i: usize) -> Option<&[u8]> {
        let layout = self.layouts.get(i)?;
        Some(&self.buf[layout.range()])
    }

    /// Get the bytes and layout of the `ith` packet in the batch.
    ///
    /// # Safety
    ///
    /// Having a mutable reference to the packet's layout allows the caller to alter the packet's
    /// owned byte range in the underlying [`Buffer`]. The caller must ensure that the layout
    /// continues to have unique ownership of the byte range it describes.
    pub unsafe fn get_data_and_layout_mut(
        &mut self,
        i: usize,
    ) -> Option<(&mut [u8], &mut PacketLayout)> {
        let layout = self.layouts.get_mut(i)?;
        let ptr = &raw const self.buf[layout.range()] as *mut [_];
        let data = unsafe { ptr.as_mut().unwrap() };
        Some((data, layout))
    }

    /// Returns the byte range of this batch in buffer, if the batch is contiguous.
    fn contiguous_range(&self) -> Option<Range<usize>> {
        if self.contiguous {
            Some(self.layouts[0].offset..self.layouts.last().unwrap().stride_end())
        } else {
            None
        }
    }

    /// Append a layout to the batch.
    ///
    /// # Safety
    ///
    /// Caller must ensure that:
    /// - The layout is within the bounds of the underlying [`Buffer`].
    /// - The layout starts after all other layouts in the batch.
    /// - No other object has ownership of the part of the [`Buffer`] described
    ///   by the layout.
    pub unsafe fn push_layout_unchecked(&mut self, layout: &PacketLayout) {
        if let Some(prev_layout) = self.layouts.last() {
            if layout.offset != prev_layout.stride_end() {
                self.contiguous = false;
            }
        } else {
            self.contiguous = true;
        }
        self.layouts.push(layout.clone());
    }

    /// Redistribute the batch's bytes into the given packet layout.
    ///
    /// The new layout can only use bytes covered by the existing layout. The new layout may
    /// abandon bytes from the old layout, which makes them permanently inaccessible.
    ///
    /// # Panics
    ///
    /// If the new layout isn't contained within the old layout.
    pub fn resegment(self, layouts: Vec<PacketLayout>) -> Self {
        if self.layouts.is_empty() {
            assert!(layouts.is_empty());
            return self;
        }
        if layouts.is_empty() {
            return self.empty_clone();
        }

        let mut contiguous = true;
        if let Some(mut range) = self.contiguous_range() {
            assert!(layouts[0].is_within(range.clone()));
            range.start = layouts[0].stride_end();
            for layout in &layouts[1..] {
                assert!(layout.is_within(range.clone()));
                contiguous &= layout.offset == range.start;
                range.start = layout.stride_end();
            }
        } else {
            // New layouts may span several old layouts, as long as those old layouts were
            // contiguous. So when validating, we want to step through each successive contiguous
            // byte range, rather than through specific old layouts.
            let mut cur_idx = 0; // in self.layouts
            let mut next_contiguous = || {
                if cur_idx >= self.layouts.len() {
                    return None;
                }

                let mut range = self.layouts[cur_idx].stride_range();
                cur_idx += 1;
                while let Some(layout) = self.layouts.get(cur_idx)
                    && layout.offset == range.end
                {
                    range.end = layout.stride_end();
                    cur_idx += 1;
                }
                Some(range)
            };

            // Edge case handling at the top of the function established that at least one old
            // and one new layout exists.
            let mut range = next_contiguous().unwrap();
            let mut prev_end = layouts[0].offset;

            for layout in &layouts {
                // Find the old range that contains this layout.
                while layout.offset >= range.end {
                    match next_contiguous() {
                        Some(next) => range = next,
                        None => panic!("layout {layout:?} references invalid region"),
                    }
                }
                assert!(layout.is_within(range.clone()));

                contiguous &= layout.offset == prev_end;
                prev_end = layout.stride_end();
                range.start = layout.stride_end();
            }
        }

        RawBatch {
            buf: self.buf,
            layouts,
            contiguous,
        }
    }

    /// Like resegment, but doesn't verify that the new layouts are legal.
    ///
    /// # Safety
    ///
    /// Caller must ensure:
    /// - All layouts are within the bounds of the underlying [`Buffer`].
    /// - The new batch has unique ownership of all parts of the [`Buffer`] covered by layouts.
    /// - Layouts are in order by increasing offset.
    /// - `contiguous` can only be true if there are no gaps between the provided layouts.
    pub unsafe fn resegment_unchecked(self, layouts: Vec<PacketLayout>, contiguous: bool) -> Self {
        RawBatch {
            buf: self.buf,
            layouts,
            contiguous,
        }
    }

    /// Make a deep copy of the batch into a new buffer.
    pub fn deep_copy(&self) -> Self {
        if let Some(range) = self.contiguous_range() {
            let buf = Buffer::from(&self.buf[range]);
            let mut offset = 0;
            let mut layouts = self.layouts.clone();
            for layout in &mut layouts {
                layout.offset = offset;
                offset += layout.stride();
            }
            return RawBatch {
                buf: Arc::new(buf),
                layouts,
                contiguous: true,
            };
        }

        let size = self.layouts.iter().map(|layout| layout.stride()).sum();
        let mut buf = Buffer::new(size);
        let mut layouts = self.layouts.clone();
        let mut offset = 0;
        for layout in &mut layouts {
            buf.data[offset..offset + layout.stride()]
                .copy_from_slice(&self.buf.data[layout.stride_range()]);
            layout.offset = offset;
            offset += layout.stride();
        }

        RawBatch {
            buf: Arc::new(buf),
            layouts,
            contiguous: true,
        }
    }
}

impl Batch for RawBatch {
    type Metadata = ();

    fn get_metadata(&self, _i: usize) -> Option<Self::Metadata> {
        Some(())
    }

    fn empty_clone(&self) -> Self {
        RawBatch {
            buf: self.buf.clone(),
            layouts: Vec::new(),
            contiguous: false,
        }
    }

    fn batch_by_mut<F, C>(mut self, mut f: F, mut collection: C) -> C
    where
        F: for<'a> FnMut(
            usize,
            crate::PacketMut<'_, Self::Metadata>,
            &'a mut C,
        ) -> Option<&'a mut Self>,
    {
        for i in 0..self.len() {
            let packet = self.index_mut(i);
            let Some(batch) = f(i, packet, &mut collection) else {
                continue;
            };
            // SAFETY: `split_retain_by_mut` consumes self, batch takes unique ownership of
            // layout's bytes.
            unsafe {
                batch.push_layout_unchecked(&self.layouts[i]);
            }
        }
        collection
    }

    // Overrides the default trait implementation, to break infinite recursion.
    fn len(&self) -> usize {
        self.layouts.len()
    }
}

impl AsRef<RawBatch> for RawBatch {
    fn as_ref(&self) -> &Self {
        self
    }
}

impl AsMut<RawBatch> for RawBatch {
    fn as_mut(&mut self) -> &mut Self {
        self
    }
}

#[cfg(test)]
mod tests {
    use alloc::{collections::BTreeMap, vec};

    use super::*;
    use crate::Batch;

    #[test]
    fn test_packet_layout() {
        let mut layout = PacketLayout {
            offset: 12,
            pre_padding: 34,
            packet_len: 56,
            post_padding: 78,
        };
        assert_eq!(layout.stride(), 34 + 56 + 78);
        assert_eq!(layout.stride_range(), 12..12 + layout.stride());
        assert_eq!(layout.start(), 12 + 34);
        assert_eq!(layout.end(), layout.start() + 56);
        assert_eq!(layout.range(), layout.start()..layout.end());
        assert!(layout.is_within(0..200));
        assert!(!layout.is_within(200..400));
        assert!(layout.is_within(layout.stride_range()));

        // Grow/shrink changes the active range, but not stride.
        let len = layout.packet_len;
        let stride = layout.stride();

        layout.resize_back(10);
        assert_eq!(layout.packet_len, len + 10);
        assert_eq!(layout.stride(), stride);

        layout.resize_front(10);
        assert_eq!(layout.packet_len, len + 20);
        assert_eq!(layout.stride(), stride);

        layout.resize_front(-5);
        assert_eq!(layout.packet_len, len + 15);
        assert_eq!(layout.stride(), stride);

        layout.resize_back(-5);
        assert_eq!(layout.packet_len, len + 10);
        assert_eq!(layout.stride(), stride);
    }

    fn mkvec(r: Range<u8>) -> Vec<u8> {
        r.collect()
    }

    fn check_packets(b: &RawBatch, packets: Vec<Vec<u8>>) {
        assert_eq!(b.len(), packets.len());
        for (a, b) in b.iter().zip(packets.into_iter()) {
            assert_eq!(a.data, b);
        }
    }

    #[test]
    fn test_batch_builder() {
        let bb: RawBatchBuilder = RawBatchBuilder::new(1000);
        let b = bb.finish();
        assert!(b.is_empty());

        let mut buf = Buffer::new(100);
        buf.data.copy_from_slice(&mkvec(0..100));
        let mut bb: RawBatchBuilder = RawBatchBuilder::from_buffer(buf);
        bb.push_layout(PacketLayout::new(0, 50));
        bb.push_layout(PacketLayout::new(50, 10));
        bb.push_layout(PacketLayout::new(80, 15));
        let b = bb.finish();
        check_packets(&b, vec![mkvec(0..50), mkvec(50..60), mkvec(80..95)]);
    }

    #[test]
    fn test_resegment() {
        let mut bb: RawBatchBuilder = RawBatchBuilder::new(100);
        bb.push_layout(PacketLayout::new(0, 100));
        let mut b = bb.finish();
        b.index_mut(0).data.copy_from_slice(&mkvec(0..100));
        let layouts = vec![
            PacketLayout::new(0, 50),
            PacketLayout::new(50, 10),
            PacketLayout::new(80, 15),
        ];
        let b = b.resegment(layouts);
        check_packets(&b, vec![mkvec(0..50), mkvec(50..60), mkvec(80..95)]);
    }

    #[test]
    fn test_deep_copy() {
        let mut bb: RawBatchBuilder = RawBatchBuilder::new(100);
        bb.push_layout(PacketLayout::new(0, 100));
        let b = bb.finish();

        let mut b2 = b.deep_copy();
        b2.index_mut(0).data.fill(42);

        assert_ne!(b.index(0).data, b2.index(0).data);
    }

    #[test]
    fn test_split_at() {
        let mut buf = Buffer::new(100);
        buf.data.copy_from_slice(&mkvec(0..100));
        let mut bb: RawBatchBuilder = RawBatchBuilder::from_buffer(buf);
        bb.push_layout(PacketLayout::new(0, 50));
        bb.push_layout(PacketLayout::new(50, 50));
        let b = bb.finish();

        let (mut b1, b2) = b.split_at(1);
        check_packets(&b1, vec![mkvec(0..50)]);
        check_packets(&b2, vec![mkvec(50..100)]);

        b1.index_mut(0).data.fill(42);

        // SAFETY: explicitly drop b1 before snarfing back the bytes with b2.
        drop(b1);
        let b2 = unsafe { b2.resegment_unchecked(vec![PacketLayout::new(0, 100)], true) };

        let mut want = mkvec(0..100);
        want[..50].fill(42);
        // Verify that b1 and b2 were still sharing a buffer, so b1's mutation is visible.
        check_packets(&b2, vec![want]);
    }

    #[test]
    fn test_batch_by() {
        let mut buf = Buffer::new(100);
        buf.data.copy_from_slice(&mkvec(0..100));
        let mut bb: RawBatchBuilder = RawBatchBuilder::from_buffer(buf);
        for i in 0..10 {
            bb.push_layout(PacketLayout::new(i * 10, 10));
        }
        let b = bb.finish();

        let empty = b.empty_clone();
        // An admittedly contrived split by whether the value of the packet's first byte has an
        // even tens digit (e.g. 25, 41, ...)
        let even_tens_digit = b.batch_by(
            |_i, packet, out| {
                let first_byte = packet.data[0];
                let tens_digit_is_even = (first_byte / 10) % 2 == 0;
                Some(
                    out.entry(tens_digit_is_even)
                        .or_insert_with(|| empty.empty_clone()),
                )
            },
            BTreeMap::new(),
        );

        check_packets(
            &even_tens_digit[&false],
            vec![
                mkvec(10..20),
                mkvec(30..40),
                mkvec(50..60),
                mkvec(70..80),
                mkvec(90..100),
            ],
        );
        check_packets(
            &even_tens_digit[&true],
            vec![
                mkvec(0..10),
                mkvec(20..30),
                mkvec(40..50),
                mkvec(60..70),
                mkvec(80..90),
            ],
        );
    }

    #[test]
    fn test_packet_reshape_mut() {
        let mut buf = Buffer::new(100);
        buf.data.copy_from_slice(&mkvec(0..100));
        let mut bb: RawBatchBuilder = RawBatchBuilder::from_buffer(buf);
        bb.push_layout(PacketLayout::with_padding(0, 50, 25, 25));
        let mut b = bb.finish();

        assert_eq!(b.len(), 1);
        assert_eq!(b.index(0).data, &mkvec(25..75));

        let mut packet = b.index_mut(0);
        assert_eq!(packet.data, &mkvec(25..75));
        packet.resize_back(10);
        assert_eq!(packet.data, &mkvec(25..85));
        packet.resize_back(-5);
        assert_eq!(packet.data, &mkvec(25..80));
        packet.resize_front(10);
        assert_eq!(packet.data, &mkvec(15..80));
        packet.resize_front(-5);
        assert_eq!(packet.data, &mkvec(20..80));

        // Resize in each dimension up to exactly the allowed amount, to check nothing panics.
        packet.resize_back(20);
        assert_eq!(packet.layout.packet_len, 80);
        assert_eq!(packet.layout.post_padding, 0);
        packet.resize_back(-80);
        assert_eq!(packet.layout.packet_len, 0);
        assert_eq!(packet.layout.post_padding, 80);
        packet.resize_front(20);
        assert_eq!(packet.layout.packet_len, 20);
        assert_eq!(packet.layout.pre_padding, 0);
        packet.resize_front(-20);
        assert_eq!(packet.layout.packet_len, 0);
        assert_eq!(packet.layout.pre_padding, 20);
    }

    #[test]
    #[should_panic]
    fn test_packet_reshape_overflow_back() {
        let mut bb = RawBatchBuilder::new(100);
        bb.push_layout(PacketLayout::with_padding(0, 50, 25, 25));
        let mut b = bb.finish();

        let mut packet = b.index_mut(0);
        packet.resize_back(26); // Panic, not enough post-padding
    }

    #[test]
    #[should_panic]
    fn test_packet_reshape_underflow_back() {
        let mut bb = RawBatchBuilder::new(100);
        bb.push_layout(PacketLayout::with_padding(0, 50, 25, 25));
        let mut b = bb.finish();

        let mut packet = b.index_mut(0);
        packet.resize_back(-51); // Panic, not enough packet body
    }

    #[test]
    #[should_panic]
    fn test_packet_reshape_overflow_front() {
        let mut bb = RawBatchBuilder::new(100);
        bb.push_layout(PacketLayout::with_padding(0, 50, 25, 25));
        let mut b = bb.finish();

        let mut packet = b.index_mut(0);
        packet.resize_front(26); // Panic, not enough pre-padding
    }

    #[test]
    #[should_panic]
    fn test_packet_reshape_underflow_front() {
        let mut bb = RawBatchBuilder::new(100);
        bb.push_layout(PacketLayout::with_padding(0, 50, 25, 25));
        let mut b = bb.finish();

        let mut packet = b.index_mut(0);
        packet.resize_front(-51); // Panic, not enough packet body
    }
}

use crate::{
    Batch, PacketLayout, PacketMut,
    raw::{RawBatch, RawBatchBuilder},
};

/// A batch of packets with no additional metadata.
pub struct BasicBatch {
    /// The underlying raw batch.
    ///
    /// You generally don't need to access this directly, except when converting `BasicBatch` into
    /// another batch type.
    pub raw: RawBatch,
}

impl From<RawBatch> for BasicBatch {
    fn from(raw: RawBatch) -> Self {
        BasicBatch { raw }
    }
}

impl AsRef<RawBatch> for BasicBatch {
    fn as_ref(&self) -> &RawBatch {
        &self.raw
    }
}

impl AsMut<RawBatch> for BasicBatch {
    fn as_mut(&mut self) -> &mut RawBatch {
        &mut self.raw
    }
}

impl Batch for BasicBatch {
    type Metadata = ();

    fn len(&self) -> usize {
        self.raw.len()
    }

    fn get_metadata(&self, _i: usize) -> Option<Self::Metadata> {
        Some(())
    }

    fn empty_clone(&self) -> Self {
        self.raw.empty_clone().into()
    }

    fn batch_by_mut<F, C>(self, mut f: F, collection: C) -> C
    where
        F: for<'a> FnMut(usize, PacketMut<'_, ()>, &'a mut C) -> Option<&'a mut Self>,
    {
        self.raw.batch_by_mut(
            |i, packet, out| {
                let basic = f(i, packet, out)?;
                Some(basic.as_mut())
            },
            collection,
        )
    }
}

impl BasicBatch {
    /// Create a batch containing a single packet of the given size, with no padding.
    pub fn new_single(size: usize) -> Self {
        let mut builder = RawBatchBuilder::new(size);
        builder.push_layout(PacketLayout::new(0, size));
        BasicBatch {
            raw: builder.finish(),
        }
    }
}

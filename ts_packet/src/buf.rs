use alloc::{vec, vec::Vec};
use core::ops::{Index, Range};

/// A contiguous span of memory that backs packet batches.
pub struct Buffer {
    /// The buffer's data.
    pub data: Vec<u8>,
}

impl Buffer {
    /// Make a new buffer of `size` bytes.
    pub fn new(size: usize) -> Self {
        Self {
            data: vec![0; size],
        }
    }
}

impl From<&[u8]> for Buffer {
    fn from(data: &[u8]) -> Self {
        Self {
            data: Vec::from(data),
        }
    }
}

impl Index<Range<usize>> for Buffer {
    type Output = [u8];

    fn index(&self, index: Range<usize>) -> &Self::Output {
        self.data.index(index)
    }
}

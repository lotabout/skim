// A ChunkList is a 2-level Vec.
// - On one hand, it could be used to reduce the realloc overhead of Vec's capacity extension
// - On the other hand, it could be cheaply cloned so that we could take snapshots while the chunk
//   list is being pushed.

use std::cmp::{max, min};
use crate::consts::{CHUNK_LIST_INIT_CAPACITY, CHUNK_SIZE};
use std::sync::{Arc};
use parking_lot::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

pub type Chunk<T> = Arc<Vec<T>>;

struct ChunkListInner<T: Clone> {
    frozen: Vec<Chunk<T>>,
    pending: Vec<T>,
}

impl<T: Clone> ChunkListInner<T> {
    fn new() -> Self {
        ChunkListInner {
            frozen: Vec::with_capacity(CHUNK_LIST_INIT_CAPACITY),
            pending: Self::new_chunk(),
        }
    }

    fn new_chunk() -> Vec<T> {
        Vec::with_capacity(CHUNK_SIZE)
    }

    fn push(&mut self, item: T) {
        if self.pending.capacity() == self.pending.len() {
            let pending_taken = std::mem::replace(&mut self.pending, Self::new_chunk());
            self.frozen.push(Arc::new(pending_taken));
        }
        self.pending.push(item);
    }
}

pub struct ChunkList<T: Clone> {
    inner: Mutex<ChunkListInner<T>>,
    len: AtomicUsize, // put len here to avoid locking mutex when all we need is length
}

impl<T: Clone> Default for ChunkList<T> {
    fn default() -> Self {
        ChunkList {
            inner: Mutex::new(ChunkListInner::new()),
            len: AtomicUsize::new(0),
        }
    }
}

impl<T: Clone> ChunkList<T> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&self, item: T) {
        let mut inner = self.inner.lock();
        inner.push(item);
        self.len.fetch_add(1, Ordering::Relaxed);
    }

    pub fn append_vec(&self, vec: Vec<T>) {
        let mut inner = self.inner.lock();
        self.len.fetch_add(vec.len(), Ordering::Relaxed);
        for item in vec.into_iter() {
            inner.push(item);
        }
    }

    pub fn clear(&self) {
        let mut inner = self.inner.lock();
        *inner = ChunkListInner::new();
    }

    pub fn snapshot(&self, start: usize) -> Vec<Chunk<T>> {
        let mut ret = Vec::new();
        let inner = self.inner.lock();

        let mut scanned = 0;
        for chunk in inner.frozen.iter() {
            if scanned > start {
                ret.push(chunk.clone());
            } else if scanned + chunk.len() > start {
                ret.push(Arc::new(Vec::from(&chunk[start - scanned..])))
            }
            scanned += chunk.len();
        }

        // copy the last chunk
        ret.push(Arc::new(Vec::from(&inner.pending[max(scanned, start) - scanned..])));
        ret
    }

    pub fn len(&self) -> usize {
        self.len.load(Ordering::Relaxed)
    }
}

mod tests {
    use super::ChunkList;
    use crate::consts::CHUNK_SIZE;

    #[test]
    fn test_push() {
        let chunk_list = ChunkList::new();
        let size = CHUNK_SIZE + CHUNK_SIZE / 2;
        for i in 0..size {
            chunk_list.push(i);
        }
        assert_eq!(size, chunk_list.len());
    }
}

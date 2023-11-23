// A ChunkList is a 2-level Vec.
// - On one hand, it could be used to reduce the realloc overhead of Vec's capacity extension
// - On the other hand, it could be cheaply cloned so that we could take snapshots while the chunk
//   list is being pushed.

use crate::consts::{CHUNK_LIST_INIT_CAPACITY, CHUNK_SIZE};
use std::sync::{Arc, Mutex};

pub type Chunk<T> = Arc<Vec<T>>;

struct ChunkListInner<T: Clone> {
    frozen: Vec<Chunk<T>>,
    pending: Chunk<T>,
}

impl<T: Clone> ChunkListInner<T> {
    fn new() -> Self {
        ChunkListInner {
            frozen: Vec::with_capacity(CHUNK_LIST_INIT_CAPACITY),
            pending: Self::new_chunk(),
        }
    }

    fn new_chunk() -> Chunk<T> {
        Arc::new(Vec::with_capacity(CHUNK_SIZE))
    }

    fn push(&mut self, item: T) {
        if self.pending.capacity() == self.pending.len() {
            self.frozen.push(self.pending.clone());
            self.pending = Self::new_chunk();
        }
        Arc::get_mut(&mut self.pending)
            .expect("could not get mut pointer for pending vec, must be bug")
            .push(item);
    }
}

pub struct ChunkList<T: Clone> {
    inner: Mutex<ChunkListInner<T>>,
}

impl<T: Clone> Default for ChunkList<T> {
    fn default() -> Self {
        ChunkList {
            inner: Mutex::new(ChunkListInner::new()),
        }
    }
}

impl<T: Clone> ChunkList<T> {
    fn new() -> Self {
        Self::default()
    }

    fn push(&self, item: T) {
        let mut inner = self.inner.lock().expect("lock failed? ask the developer");
        inner.push(item);
    }

    fn clear(&self) {
        let mut inner = self.inner.lock().expect("lock failed? ask the developer");
        *inner = ChunkListInner::new();
    }

    fn snapshot(&self) -> Vec<Chunk<T>> {
        let inner = self.inner.lock().expect("lock failed? ask the developer");
        let mut ret = inner.frozen.clone();
        // copy the last chunk
        ret.push(Arc::new((*inner.pending).clone()));
        ret
    }

    fn len(&self) -> usize {
        let inner = self.inner.lock().expect("lock failed? ask the developer");
        inner.frozen.iter().map(|c| c.len()).sum::<usize>() + inner.pending.len()
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

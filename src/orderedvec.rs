// ordered container
// Normally, user will only care about the first several options. So we only keep several of them
// in order. Other items are kept unordered and are sorted on demand.

use rayon::prelude::*;
use std::cmp::Ordering;

pub type CompareFunction<T> = Box<Fn(&T, &T) -> Ordering + Send + Sync>;
const ORDERED_SIZE: usize = 300;

pub struct OrderedVec<T: Send> {
    vec: Vec<T>,
    compare: CompareFunction<T>,
}

impl<T: Send> OrderedVec<T> {
    pub fn new(compare: CompareFunction<T>) -> Self {
        OrderedVec {
            vec: Vec::with_capacity(ORDERED_SIZE),
            compare,
        }
    }

    pub fn append_ordered(&mut self, mut items: Vec<T>) {
        self.vec.append(&mut items);
        self.vec.par_sort_unstable_by(self.compare.as_ref());
    }

    pub fn get(&self, index: usize) -> Option<&T> {
        self.vec.get(index)
    }

    pub fn len(&self) -> usize {
        self.vec.len()
    }

    pub fn clear(&mut self) {
        self.vec.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.vec.is_empty()
    }

    pub fn iter<'a>(&'a self) -> Box<Iterator<Item = &T> + 'a> {
        Box::new(self.vec.iter())
    }
}

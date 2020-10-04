// ordered container
// Normally, user will only care about the first several options. So we only keep several of them
// in order. Other items are kept unordered and are sorted on demand.

use defer_drop::DeferDrop;
use rayon::prelude::ParallelSliceMut;
use std::cell::{Ref, RefCell};
use std::cmp::Ordering;

const ORDERED_SIZE: usize = 300;
const MAX_MOVEMENT: usize = 100;

pub struct OrderedVec<T: Send + Ord + 'static> {
    // sorted vectors for merge, reverse ordered, last one is the smallest one
    sub_vectors: RefCell<DeferDrop<Vec<Vec<T>>>>,
    // globally sorted items, the first one is the smallest one.
    sorted: RefCell<DeferDrop<Vec<T>>>,
    tac: bool,
    nosort: bool,
}

impl<T: Send + Ord + 'static> OrderedVec<T> {
    pub fn new() -> Self {
        OrderedVec {
            sub_vectors: RefCell::new(DeferDrop::new(Vec::new())),
            sorted: RefCell::new(DeferDrop::new(Vec::with_capacity(ORDERED_SIZE))),
            tac: false,
            nosort: false,
        }
    }

    pub fn tac(&mut self, tac: bool) -> &mut Self {
        self.tac = tac;
        self
    }

    pub fn nosort(&mut self, nosort: bool) -> &mut Self {
        self.nosort = nosort;
        self
    }

    pub fn append(&mut self, mut items: Vec<T>) {
        trace!("orderedvec append: new vec size: {}", items.len());
        if self.nosort {
            self.sorted.borrow_mut().append(&mut items);
            return;
        }

        self.sort_vector(&mut items, false);
        let mut sorted = self.sorted.borrow_mut();

        let mut items_smaller = Vec::new();
        if !sorted.is_empty() {
            // move the ones <= sorted to sorted
            while items_smaller.len() < MAX_MOVEMENT
                && !items.is_empty()
                && self.compare_item(items.last().unwrap(), sorted.last().unwrap()) == Ordering::Less
            {
                items_smaller.push(items.pop().unwrap());
            }
        }

        if !items.is_empty() {
            self.sub_vectors.borrow_mut().push(items);
        }

        let too_many_moved = items_smaller.len() >= ORDERED_SIZE;
        trace!("append_ordered: num_moved: {}", items_smaller.len());

        sorted.append(&mut items_smaller);
        if too_many_moved {
            // means the current sorted vector contains item that's large
            // so we'll move the sorted vector to partially sorted candidates.
            self.sort_vector(&mut sorted, false);
            let old_vec = self.sorted.replace(DeferDrop::new(Vec::new()));
            self.sub_vectors.borrow_mut().push(DeferDrop::into_inner(old_vec));
        } else {
            self.sort_vector(&mut sorted, true);
        }

        trace!(
            "orderedvec done append: sub_vector size: {}",
            self.sub_vectors.borrow().len()
        );
    }

    fn sort_vector(&self, vec: &mut Vec<T>, asc: bool) {
        let asc = asc ^ self.tac;
        vec.par_sort();
        if !asc {
            vec.reverse();
        }
    }

    #[inline]
    fn compare_item(&self, a: &T, b: &T) -> Ordering {
        if !self.tac {
            a.cmp(b)
        } else {
            b.cmp(a)
        }
    }

    fn merge_till(&self, index: usize) {
        let mut sorted = self.sorted.borrow_mut();
        let mut vectors = self.sub_vectors.borrow_mut();

        if index >= sorted.len() {
            trace!("merge_till: index: {}, num_sorted: {}", index, sorted.len());
        }

        while index >= sorted.len() {
            let o_min_index = vectors
                .iter()
                .map(|v| v.last())
                .enumerate()
                .filter(|(_idx, item)| item.is_some())
                .min_by(|(_, a), (_, b)| self.compare_item(a.unwrap(), b.unwrap()))
                .map(|(idx, _)| idx);
            if o_min_index.is_none() {
                break;
            }

            let min_index = o_min_index.unwrap();
            let min_item = vectors[min_index].pop();
            if min_item.is_none() {
                break;
            }

            if vectors[min_index].is_empty() {
                vectors.remove(min_index);
            }

            sorted.push(min_item.unwrap());
        }
    }

    pub fn get(&self, index: usize) -> Option<Ref<T>> {
        self.merge_till(index);
        if self.len() <= index {
            None
        } else {
            let index = if self.tac && self.nosort {
                self.len() - index - 1
            } else {
                index
            };
            Some(Ref::map(self.sorted.borrow(), |list| &list[index]))
        }
    }

    pub fn len(&self) -> usize {
        let sorted_len = self.sorted.borrow().len();
        let unsorted_len: usize = self.sub_vectors.borrow().iter().map(|v| v.len()).sum();
        sorted_len + unsorted_len
    }

    pub fn clear(&mut self) {
        self.sub_vectors.replace(DeferDrop::new(Vec::new()));
        self.sorted.replace(DeferDrop::new(Vec::new()));
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn iter(&self) -> impl Iterator<Item = Ref<T>> {
        self.merge_till(self.len());
        OrderedVecIter {
            ordered_vec: self,
            index: 0,
        }
    }
}

struct OrderedVecIter<'a, T: Send + Ord + 'static> {
    ordered_vec: &'a OrderedVec<T>,
    index: usize,
}

impl<'a, T: Send + Ord + 'static> Iterator for OrderedVecIter<'a, T> {
    type Item = Ref<'a, T>;

    fn next(&mut self) -> Option<Self::Item> {
        let ret = self.ordered_vec.get(self.index);
        self.index += 1;
        ret
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test() {
        let a = vec![1, 3, 5, 7];
        let b = vec![4, 8, 9];
        let c = vec![2, 6, 10];
        let mut ordered_vec = OrderedVec::new();
        ordered_vec.append(a);
        assert_eq!(*ordered_vec.get(0).unwrap(), 1);

        ordered_vec.append(b);
        assert_eq!(*ordered_vec.get(1).unwrap(), 3);
        assert_eq!(*ordered_vec.get(2).unwrap(), 4);
        assert_eq!(*ordered_vec.get(3).unwrap(), 5);

        ordered_vec.append(c);

        for (idx, item) in ordered_vec.iter().enumerate() {
            assert_eq!(idx + 1, *item)
        }
    }

    #[test]
    fn test_tac() {
        let a = vec![1, 3, 5, 7];
        let b = vec![4, 8, 9];
        let c = vec![2, 6, 10];
        let mut ordered_vec = OrderedVec::new();
        ordered_vec.tac(true);

        ordered_vec.append(a);
        assert_eq!(*ordered_vec.get(0).unwrap(), 7);

        ordered_vec.append(b);
        assert_eq!(*ordered_vec.get(1).unwrap(), 8);
        assert_eq!(*ordered_vec.get(2).unwrap(), 7);
        assert_eq!(*ordered_vec.get(3).unwrap(), 5);

        ordered_vec.append(c);
        for (idx, item) in ordered_vec.iter().enumerate() {
            assert_eq!(10 - idx, *item)
        }
    }

    #[test]
    fn test_nosort() {
        let a = vec![1, 3, 5, 7];
        let b = vec![4, 8, 9];
        let c = vec![2, 6, 10];
        let d = vec![1, 3, 5, 7, 4, 8, 9, 2, 6, 10];
        let mut ordered_vec = OrderedVec::new();
        ordered_vec.nosort(true);
        ordered_vec.append(a);
        ordered_vec.append(b);
        ordered_vec.append(c);
        for (a, b) in ordered_vec.iter().zip(d.iter()) {
            assert_eq!(*a, *b);
        }
    }

    #[test]
    fn test_nosort_and_tac() {
        let a = vec![1, 3, 5, 7];
        let b = vec![4, 8, 9];
        let c = vec![2, 6, 10];
        let d = vec![10, 6, 2, 9, 8, 4, 7, 5, 3, 1];
        let mut ordered_vec = OrderedVec::new();
        ordered_vec.nosort(true).tac(true);
        ordered_vec.append(a);
        ordered_vec.append(b);
        ordered_vec.append(c);
        for (a, b) in ordered_vec.iter().zip(d.iter()) {
            assert_eq!(*a, *b);
        }
    }

    #[test]
    fn test_equals() {
        let a = vec![1, 2, 3, 4];
        let b = vec![5, 6, 7, 8];
        let target = vec![1, 2, 3, 4, 5, 6, 7, 8];
        let mut ordered_vec = OrderedVec::new();
        ordered_vec.append(a);
        ordered_vec.append(b);
        for (a, b) in ordered_vec.iter().zip(target.iter()) {
            assert_eq!(*a, *b);
        }
    }
}

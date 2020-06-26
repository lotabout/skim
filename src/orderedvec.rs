// ordered container
// Normally, user will only care about the first several options. So we only keep several of them
// in order. Other items are kept unordered and are sorted on demand.

use std::cell::{Ref, RefCell};
use std::cmp::Ordering;

pub type CompareFunction<T> = Box<dyn Fn(&T, &T) -> Ordering + Send + Sync>;

const ORDERED_SIZE: usize = 300;
const MAX_MOVEMENT: usize = 100;

pub struct OrderedVec<T: Send> {
    // sorted vectors for merge, reverse ordered, last one is the smallest one
    sub_vectors: RefCell<Vec<Vec<T>>>,
    // globally sorted items, the first one is the smallest one.
    sorted: RefCell<Vec<T>>,
    compare: CompareFunction<T>,
}

impl<T: Send> OrderedVec<T> {
    pub fn new(compare: CompareFunction<T>) -> Self {
        OrderedVec {
            sub_vectors: RefCell::new(Vec::new()),
            sorted: RefCell::new(Vec::with_capacity(ORDERED_SIZE)),
            compare,
        }
    }

    pub fn append_ordered(&mut self, mut items: Vec<T>) {
        items.sort_by(|a, b| (self.compare)(b, a));
        let mut sorted = self.sorted.borrow_mut();

        let mut items_smaller = Vec::new();
        if !sorted.is_empty() {
            // move the ones <= sorted to sorted
            while items_smaller.len() < MAX_MOVEMENT
                && !items.is_empty()
                && (self.compare)(items.last().unwrap(), sorted.last().unwrap()) == Ordering::Less
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
            sorted.sort_by(|a, b| (self.compare)(b, a));
            self.sub_vectors.borrow_mut().push(self.sorted.replace(Vec::new()));
        } else {
            sorted.sort_by(self.compare.as_ref());
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
                .min_by(|(_, a), (_, b)| {
                    if a.is_none() || b.is_none() {
                        Ordering::Greater
                    } else {
                        (self.compare)(a.unwrap(), b.unwrap())
                    }
                })
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
            Some(Ref::map(self.sorted.borrow(), |list| &list[index]))
        }
    }

    pub fn len(&self) -> usize {
        let sorted_len = self.sorted.borrow().len();
        let unsorted_len: usize = self.sub_vectors.borrow().iter().map(|v| v.len()).sum();
        sorted_len + unsorted_len
    }

    pub fn clear(&mut self) {
        self.sub_vectors.borrow_mut().clear();
        self.sorted.borrow_mut().clear();
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn get_sorted(&self) -> Ref<Vec<T>> {
        self.merge_till(self.len());
        self.sorted.borrow()
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
        let mut ordered_vec = OrderedVec::new(Box::new(usize::cmp));
        ordered_vec.append_ordered(a);
        assert_eq!(*ordered_vec.get(0).unwrap(), 1);

        ordered_vec.append_ordered(b);
        assert_eq!(*ordered_vec.get(1).unwrap(), 3);
        assert_eq!(*ordered_vec.get(2).unwrap(), 4);
        assert_eq!(*ordered_vec.get(3).unwrap(), 5);

        ordered_vec.append_ordered(c);
        assert_eq!(*ordered_vec.get(0).unwrap(), 1);
        assert_eq!(*ordered_vec.get(1).unwrap(), 2);
        assert_eq!(*ordered_vec.get(2).unwrap(), 3);
        assert_eq!(*ordered_vec.get(5).unwrap(), 6);
        assert_eq!(*ordered_vec.get(9).unwrap(), 10);
        assert!(ordered_vec.get(10).is_none());
    }
}

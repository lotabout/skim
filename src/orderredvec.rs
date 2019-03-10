// ordered container
// Normally, user will only care about the first several options. So we only keep several of them
// in order. Other items are kept unordered and are sorted on demand.

const ORDERED_SIZE: usize = 300;
use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Clone)]
pub struct OrderedVec<T: Ord> {
    ordered: RefCell<Vec<T>>,
    unordered: RefCell<Vec<T>>,
    sorted: AtomicBool,
}

impl<T> OrderedVec<T>
where
    T: Ord,
{
    pub fn new() -> Self {
        OrderedVec {
            ordered: RefCell::new(Vec::with_capacity(ORDERED_SIZE)),
            unordered: RefCell::new(Vec::with_capacity(ORDERED_SIZE * 2)),
            sorted: AtomicBool::new(false),
        }
    }

    fn ordered_insert(&mut self, item: T) {
        self.ordered.push(item);
        let mut pos = self.ordered.len() - 1;
        while pos > 0 && self.ordered[pos] < self.ordered[pos - 1] {
            self.ordered.borrow_mut().swap(pos, pos - 1);
            pos -= 1;
        }
    }

    pub fn push(&mut self, item: T) {
        if self.ordered.len() < ORDERED_SIZE {
            self.ordered_insert(item);
            return;
        }

        let smaller = if item > *self.ordered.last().expect("orderedvec: failed to get last element") {
            item
        } else {
            self.ordered_insert(item);
            self.ordered.pop().expect("orderedvec: No element at all!")
        };

        self.unordered.push(smaller);
        self.sorted.store(false, Ordering::Relaxed);
    }

    pub fn get(&self, index: usize) -> Option<&T> {
        if index < self.ordered.len() {
            return self.ordered.get(index);
        }

        if index >= self.ordered.len() + self.unordered.len() {
            return None;
        }

        if !self.sorted.load(Ordering::Relaxed) {
            self.unordered.sort_by(|a, b| a.cmp(b));
            self.sorted.store(true, Ordering::Relaxed);
        }

        self.unordered.get(index - self.ordered.len())
    }

    pub fn len(&self) -> usize {
        self.ordered.len() + self.unordered.len()
    }

    pub fn clear(&mut self) {
        self.ordered.clear();
        self.unordered.clear();
        self.sorted.store(true, Ordering::Relaxed);
    }

    pub fn iter<'a>(&'a self) -> Box<Iterator<Item = &T> + 'a> {
        let ordered = &self.ordered;
        let unordered = &self.unordered;
        Box::new(ordered.iter().chain(unordered.iter()))
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_push_get() {
        let mut ordered = OrderedVec::new();
        let data = [1, 3, -2, -1, 0, 4, 5];
        for i in data.iter() {
            ordered.push(*i);
        }

        println!("{}", ordered.get(0).unwrap());

        //assert_eq!(*ordered.get(0).unwrap(), -2);
        assert_eq!(*(ordered.get(0).unwrap()), -2);
        assert_eq!(*(ordered.get(1).unwrap()), -1);
        assert_eq!(*(ordered.get(2).unwrap()), 0);
        assert_eq!(*(ordered.get(3).unwrap()), 1);
        assert_eq!(*(ordered.get(4).unwrap()), 3);
        assert_eq!(*(ordered.get(5).unwrap()), 4);
        assert_eq!(*(ordered.get(6).unwrap()), 5);
    }
}

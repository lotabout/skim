// ordered container

use std::collections::BinaryHeap;

pub struct OrderedVec<T: Ord> {
    heap: BinaryHeap<T>,
    vec: Vec<T>,
    sorted: bool,
    index_min: usize,
}

impl<T> OrderedVec<T> where T: Ord{
    pub fn new() -> Self {
        OrderedVec {
            heap: BinaryHeap::new(),
            vec: Vec::new(),
            sorted: true,
            index_min: 0,
        }
    }

    pub fn push(&mut self, item: T) {
        if self.index_min > 0 && item > self.vec[self.index_min-1] {
            self.vec.push(item);
            self.sorted = false;
        } else {
            self.heap.push(item);
        }
    }

    pub fn get(&mut self, index: usize) -> Option<&T> {
        if !self.sorted {
            self.vec.sort();
        }

        if index >= self.vec.len() + self.heap.len() {
            return None;
        }

        let mut len = self.vec.len();
        while len <= index {
            self.vec.push(self.heap.pop().unwrap());
            len += 1;
        }

        self.sorted = true;
        self.index_min = self.vec.len();
        return self.vec.get(index);
    }

    pub fn len(&self) -> usize {
        return self.vec.len() + self.heap.len();
    }

    pub fn clear(&mut self) {
        self.vec.clear();
        self.heap.clear();
        self.sorted = true;
        self.index_min = 0;
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_push_get() {
        let mut ordered = OrderedVec::new();
        let data = [1,3,-2,-1,0,4,5];
        for i in data.iter() {
            ordered.push(*i);
        }

        println!("{}", ordered.get(0).unwrap());

        //assert_eq!(*ordered.get(0).unwrap(), -2);
        assert_eq!(*(ordered.get(0).unwrap()), 5);
        assert_eq!(*(ordered.get(1).unwrap()), 4);
        assert_eq!(*(ordered.get(2).unwrap()), 3);
        assert_eq!(*(ordered.get(3).unwrap()), 1);
        assert_eq!(*(ordered.get(4).unwrap()), 0);
        assert_eq!(*(ordered.get(5).unwrap()), -1);
        assert_eq!(*(ordered.get(6).unwrap()), -2);
    }
}

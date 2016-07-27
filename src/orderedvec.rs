// ordered container
// Normally, user will only care about the first several options. So we only keep several of them
// in order. Other items are kept unordered and are sorted on demand.

const ORDERED_SIZE: usize = 300;

#[derive(Clone)]
pub struct OrderedVec<T: Ord> {
    ordered: Vec<T>,
    unordered: Vec<T>,
    sorted: bool,
}

impl<T> OrderedVec<T> where T: Ord {
    pub fn new() -> Self {
        OrderedVec {
            ordered: Vec::new(),
            unordered: Vec::new(),
            sorted: false,
        }
    }

    fn ordered_insert(&mut self, item: T) {
        self.ordered.push(item);
        let mut pos = self.ordered.len() - 1;
        while pos > 0 && self.ordered[pos] < self.ordered[pos-1] {
            self.ordered.swap(pos, pos-1);
            pos -= 1;
        }
    }

    pub fn push(&mut self, item: T) {
        if self.ordered.len() < ORDERED_SIZE {
            self.ordered_insert(item);
            return;
        }

        let smaller = if item > *self.ordered.last().unwrap() {
            item
        } else {
            self.ordered_insert(item);
            self.ordered.pop().unwrap()
        };

        self.unordered.push(smaller);
        self.sorted = false;
    }

    pub fn get(&mut self, index: usize) -> Option<&T> {
        if index < self.ordered.len() {
            return self.ordered.get(index);
        }

        if index >= self.ordered.len() + self.unordered.len() {
            return None;
        }

        if !self.sorted {
            self.unordered.sort_by(|a, b| a.cmp(b));
            self.sorted = true;
        }

        self.unordered.get(index - self.ordered.len())
    }

    pub fn len(&self) -> usize {
        self.ordered.len() + self.unordered.len()
    }

    pub fn clear(&mut self) {
        self.ordered.clear();
        self.unordered.clear();
        self.sorted = true;
    }

    pub fn iter<'a>(&'a self) -> Box<Iterator<Item=&'a T> + 'a> {
        let ref ordered = self.ordered;
        let ref unordered = self.unordered;
        Box::new(ordered.iter().chain(unordered.iter()))
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
        assert_eq!(*(ordered.get(0).unwrap()), -2);
        assert_eq!(*(ordered.get(1).unwrap()), -1);
        assert_eq!(*(ordered.get(2).unwrap()), 0);
        assert_eq!(*(ordered.get(3).unwrap()), 1);
        assert_eq!(*(ordered.get(4).unwrap()), 3);
        assert_eq!(*(ordered.get(5).unwrap()), 4);
        assert_eq!(*(ordered.get(6).unwrap()), 5);
    }
}

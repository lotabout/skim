// ordered container
// Normally, user will only care about the first several options. So we only keep several of them
// in order. Other items are kept unordered and are sorted on demand.

const ORDERED_SIZE: usize = 300;
use rayon::prelude::*;

pub struct OrderedVec<T: Ord> {
    vec: Vec<T>,
}

impl<T> OrderedVec<T>
where
    T: Ord,
{
    pub fn new() -> Self {
        OrderedVec {
            vec: Vec::with_capacity(ORDERED_SIZE),
        }
    }

    pub fn append_ordered(&mut self, mut items: Vec<T>) {
        if self.vec.is_empty() {
            self.vec = items;
        } else {
            self.vec.append(&mut items);
            self.vec.sort_unstable();
        }
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

// ordered container
// Normally, user will only care about the first several options. So we only keep several of them
// in order. Other items are kept unordered and are sorted on demand.

const ORDERED_SIZE: usize = 300;

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

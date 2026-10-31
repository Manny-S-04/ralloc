use crate::{Memory, align_up};

pub struct BumpAllocator<const HEAP_SIZE: usize> {
    heap: Memory<HEAP_SIZE>,
    next: usize,
    count: usize,
}

impl<const HEAP_SIZE: usize> Default for BumpAllocator<HEAP_SIZE> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const HEAP_SIZE: usize> BumpAllocator<HEAP_SIZE> {
    pub const fn new() -> Self {
        Self {
            heap: Memory([0; HEAP_SIZE]),
            next: 0,
            count: 0,
        }
    }

    pub fn ralloc(&mut self, size: usize, align: usize) -> Option<*mut u8> {
        if !align.is_power_of_two() {
            return None;
        }
        let base = self.heap.0.as_mut_ptr();
        let start = align_up(base as usize + self.next, align) - base as usize;
        let end = start.checked_add(size)?;
        // check before bumping
        if end > HEAP_SIZE {
            return None;
        }

        self.next = end;
        self.count += 1;
        Some(unsafe { base.add(start) })
    }

    pub const fn used(&self) -> usize {
        self.next
    }

    pub const fn count(&self) -> usize {
        self.count
    }

    pub fn reset(&mut self) {
        for i in 0..HEAP_SIZE {
            self.heap.0[i] = 0;
        }
        self.next = 0;
        self.count = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aligned() {
        let mut b = BumpAllocator::<256>::new();
        b.ralloc(1, 1).unwrap();
        let p = b.ralloc(8, 8).unwrap();
        assert_eq!(p as usize % 8, 0);
        let q = b.ralloc(4, 64).unwrap();
        assert_eq!(q as usize % 64, 0);
    }

    #[test]
    fn full() {
        let mut b = BumpAllocator::<64>::new();
        assert!(b.ralloc(60, 1).is_some());
        let used = b.used();
        assert!(b.ralloc(8, 1).is_none());
        assert_eq!(b.used(), used);
        assert!(b.ralloc(4, 1).is_some());
        assert!(b.ralloc(usize::MAX, 1).is_none());
        assert!(b.ralloc(1, 3).is_none());
    }

    #[test]
    fn reset() {
        let mut b = BumpAllocator::<64>::new();
        let p = b.ralloc(64, 1).unwrap();
        unsafe { *p = 7 };
        b.reset();
        assert_eq!(b.used(), 0);
        assert_eq!(b.count(), 0);
        let q = b.ralloc(64, 1).unwrap();
        assert_eq!(p, q);
        assert_eq!(unsafe { *q }, 0);
    }
}

use crate::Memory;
use crate::freelist::{self, FreeList};
use std::alloc::{GlobalAlloc, Layout};
use std::cell::UnsafeCell;
use std::hint::spin_loop;
use std::ptr::{copy_nonoverlapping, null_mut};
use std::sync::atomic::{AtomicBool, Ordering};

pub struct Ralloc<const N: usize> {
    locked: AtomicBool,
    ready: UnsafeCell<bool>,
    list: UnsafeCell<FreeList>,
    heap: UnsafeCell<Memory<N>>,
}

unsafe impl<const N: usize> Sync for Ralloc<N> {}

impl<const N: usize> Default for Ralloc<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize> Ralloc<N> {
    pub const fn new() -> Self {
        Self {
            locked: AtomicBool::new(false),
            ready: UnsafeCell::new(false),
            list: UnsafeCell::new(FreeList::empty()),
            heap: UnsafeCell::new(Memory([0; N])),
        }
    }

    // spinlock, a Mutex could allocate
    fn with<T>(&self, f: impl FnOnce(&mut FreeList) -> T) -> T {
        while self
            .locked
            .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            spin_loop();
        }

        let fl = unsafe { &mut *self.list.get() };
        let ready = unsafe { &mut *self.ready.get() };
        if !*ready {
            let base = self.heap.get().cast::<u8>();
            unsafe { freelist::init(fl, base, N) };
            *ready = true;
        }
        let result = f(fl);

        self.locked.store(false, Ordering::Release);
        result
    }

    pub fn stats(&self) -> freelist::Stats {
        self.with(|fl| freelist::stats(fl))
    }

    pub fn check(&self) -> Result<(), String> {
        self.with(|fl| freelist::check(fl))
    }
}

unsafe impl<const N: usize> GlobalAlloc for Ralloc<N> {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        self.with(|fl| freelist::ralloc(fl, layout.size(), layout.align()))
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        self.with(|fl| unsafe { freelist::free_sized(fl, ptr, layout.size()) })
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let usable = self.with(|fl| unsafe { freelist::usable_size(fl, ptr) });
        if new_size <= usable {
            return ptr;
        }

        let new = self.with(|fl| freelist::ralloc(fl, new_size, layout.align()));
        if new.is_null() {
            return null_mut();
        }
        unsafe {
            copy_nonoverlapping(ptr, new, layout.size());
            self.dealloc(ptr, layout);
        }
        new
    }
}

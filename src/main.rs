#[allow(unused)]
use std::ptr::null_mut;

#[derive(Copy, Clone)]
struct HeapChunk {
    ptr: *mut u8,
    size: usize,
}

impl HeapChunk {
    pub const fn new() -> Self {
        Self {
            ptr: null_mut(),
            size: 0,
        }
    }
}

pub struct BumpAllocator<const HEAP_SIZE: usize> {
    heap: [u8; HEAP_SIZE],
    heap_chunks: [HeapChunk; HEAP_SIZE],
    next: usize,
    chunk_next: usize,
}

impl<const HEAP_SIZE: usize> BumpAllocator<HEAP_SIZE> {
    pub const fn new() -> Self {
        Self {
            heap: [0; HEAP_SIZE],
            heap_chunks: [HeapChunk::new(); HEAP_SIZE],
            next: 0,
            chunk_next: 0,
        }
    }

    pub fn ralloc(&mut self, size: usize) -> Option<*mut u8> {
        if self.next + size > HEAP_SIZE {
            return None;
        }

        let ptr = unsafe {
            self.heap.as_mut_ptr().add(self.next)
        };

        self.next += size;

        if self.chunk_next + size > HEAP_SIZE {
            return None;
        }

        let chunk = HeapChunk{
            ptr,
            size
        };

        self.heap_chunks[self.chunk_next] = chunk;
        self.chunk_next += 1;

        Some(ptr)
    }

    pub unsafe fn print_chunks(&mut self) {
        for i in 0..self.chunk_next {
            let chunk = self.heap_chunks[i];
            println!("{:p}: {}", chunk.ptr, chunk.size);
        }
    }

    pub unsafe fn print_heap(&mut self, start: *mut u8, len: usize) {
        unsafe {
            let ptr = self.heap.as_mut_ptr();
            for i in 0..self.heap.len() {
                if !(ptr.add(i) == start) {
                    continue
                }
                for i in 0..len {
                    let byte_ptr = start.add(i);
                    println!("{:p}: {}", byte_ptr, *byte_ptr);
                }
            }
        }
    }

    pub fn reset(&mut self) {
        for i in 0..self.heap.len() {
            self.heap[i] = 0;
        }
        self.next = 0;
    }
}

fn main() {
    let mut b = BumpAllocator::<1024>::new();
    let size: usize = 16;

    unsafe {
        if let Some(ptr) = b.ralloc(size) {
            for i in 0..size {
                *ptr.add(i) = ('A' as u8) + (i as u8); 
            }
        } else {
            println!("Not enough mem allocated");
        }

        b.print_chunks();
    }
}

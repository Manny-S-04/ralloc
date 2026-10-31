# ralloc

Memory allocator in Rust, written to learn what malloc actually does. It started as a bump allocator and is now a free list allocator that can be the `#[global_allocator]`.

## Run

```sh
cargo run    # allocates a few blocks, frees them, prints the heap after each step
cargo test
cargo +nightly miri test
```

```rust
let mut mem = Memory([0; 1024]);
let mut fl = FreeList::empty();
unsafe { init(&mut fl, mem.0.as_mut_ptr(), 1024) };

let p = ralloc(&mut fl, 100, 16);
unsafe { free(&mut fl, p) };
```

```rust
#[global_allocator]
static GLOBAL: Ralloc<{ 16 * 1024 * 1024 }> = Ralloc::new();
```

## Layout

- `src/bump.rs` - the original bump allocator, now aligned. `reset()` frees everything at once like an arena
- `src/freelist.rs` - the real allocator
- `src/global.rs` - `GlobalAlloc` on top, behind a spinlock
- `tests/global.rs` - Vec, String, HashMap, threads and 4096 aligned boxes running on it

## How it works

The heap is one block of memory, 16 byte aligned. Every block has an 8 byte header and an 8 byte footer, both holding the block size with the low bit set if it's in use. Sizes are multiples of 16 so the low bits are free for flags, and payloads always land on 16 byte boundaries.

```
[used footer][hdr | payload | ftr][hdr | payload | ftr] ... [used header, size 0]
```

The used footer at the start and the size 0 header at the end mean coalescing never walks off either end.

Free blocks also keep next and prev offsets in their payload, which makes a doubly linked free list. Offsets from the heap base instead of pointers, so there are no pointers stored inside the heap.

- alloc: walks the free list and takes the first block big enough (first fit). If what's left is at least 32 bytes it gets split off as a new free block.
- alignment above 16: finds an aligned spot inside a free block. The gap in front has to be 0 or at least 32 bytes so it can become its own free block.
- free: looks at the footer just before the block and the header just after. If either neighbour is free it gets unlinked and merged (coalescing), so two free blocks are never next to each other.
- realloc: returns the same pointer if the block already has room, otherwise alloc, copy, free.

`check()` walks every block and the free list and returns an error if a header and footer disagree, two free blocks touch, or the list doesn't match the heap. The tests call it after nearly every step, including a 5000 step random alloc/free run that also checks no allocation was overwritten.

## Things I got caught on

- the old bump allocator bumped `next` before checking the second bound, so a failed alloc still used up space. `reset` also didn't reset the chunk count
- the old chunk records were two `[HeapChunk; HEAP_SIZE]` arrays, 16 bytes of metadata per byte of heap each, so 32x overhead. Headers inline in the heap fix that
- `[u8; N]` is only 1 byte aligned, the heap needs `#[repr(align(16))]`
- Miri caught a real bug in `GlobalAlloc`. When a `Box` is dropped it's still borrowed until `dealloc` returns, and `free` was writing the free list links into that memory through the heap's base pointer. `free_sized` writes those bytes through the pointer it was given instead, which is why `dealloc` passes the size
- a `Mutex` inside the allocator could allocate, so it's a spinlock on an `AtomicBool`

## Todo

- [x] bump allocation
- [x] alignment
- [x] free list, first fit, splitting
- [x] coalescing free chunks
- [x] headers inline in the heap instead of the two big arrays
- [x] `GlobalAlloc` and run Vec/String/HashMap on it
- [x] miri
- [ ] get memory with mmap and grow the heap
- [ ] size classes so small allocs don't walk the whole list
- [ ] grow in place in realloc when the next block is free
- [ ] benchmark against the system allocator

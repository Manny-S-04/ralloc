use crate::align_up;
use std::ptr::null_mut;

pub const ALIGN: usize = 16;
pub const WORD: usize = 8;
pub const OVERHEAD: usize = 2 * WORD;
pub const MIN_BLOCK: usize = 32;
pub const MIN_HEAP: usize = 64;
const NIL: usize = usize::MAX;
const USED: usize = 1;

pub struct FreeList {
    base: *mut u8,
    len: usize,
    head: usize,
}

impl Default for FreeList {
    fn default() -> Self {
        Self::empty()
    }
}

impl FreeList {
    pub const fn empty() -> Self {
        Self {
            base: null_mut(),
            len: 0,
            head: NIL,
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Stats {
    pub used_blocks: usize,
    pub free_blocks: usize,
    pub used_bytes: usize,
    pub free_bytes: usize,
    pub largest_free: usize,
}

unsafe fn get(fl: &FreeList, off: usize) -> usize {
    unsafe { fl.base.add(off).cast::<usize>().read() }
}

unsafe fn set(fl: &FreeList, off: usize, value: usize) {
    unsafe { fl.base.add(off).cast::<usize>().write(value) }
}

unsafe fn block_size(fl: &FreeList, h: usize) -> usize {
    unsafe { get(fl, h) & !USED }
}

unsafe fn is_used(fl: &FreeList, h: usize) -> bool {
    unsafe { get(fl, h) & USED != 0 }
}

unsafe fn set_block(fl: &FreeList, h: usize, size: usize, used: bool) {
    let tag = size | if used { USED } else { 0 };
    unsafe {
        set(fl, h, tag);
        set(fl, h + size - WORD, tag);
    }
}

unsafe fn next_free(fl: &FreeList, h: usize) -> usize {
    unsafe { get(fl, h + WORD) }
}

unsafe fn prev_free(fl: &FreeList, h: usize) -> usize {
    unsafe { get(fl, h + 2 * WORD) }
}

unsafe fn push(fl: &mut FreeList, h: usize) {
    unsafe {
        set(fl, h + WORD, fl.head);
        set(fl, h + 2 * WORD, NIL);
        if fl.head != NIL {
            set(fl, fl.head + 2 * WORD, h);
        }
    }
    fl.head = h;
}

unsafe fn unlink(fl: &mut FreeList, h: usize) {
    unsafe {
        let next = next_free(fl, h);
        let prev = prev_free(fl, h);
        if prev == NIL {
            fl.head = next;
        } else {
            set(fl, prev + WORD, next);
        }
        if next != NIL {
            set(fl, next + 2 * WORD, prev);
        }
    }
}

pub const fn block_for(size: usize) -> Option<usize> {
    let Some(with_tags) = size.checked_add(OVERHEAD + ALIGN - 1) else {
        return None;
    };
    let size = with_tags & !(ALIGN - 1);
    if size < MIN_BLOCK {
        Some(MIN_BLOCK)
    } else {
        Some(size)
    }
}

// safety: base is valid for len bytes, 16 aligned and only used through fl
pub unsafe fn init(fl: &mut FreeList, base: *mut u8, len: usize) -> bool {
    let len = len & !(ALIGN - 1);
    if base.is_null() || !(base as usize).is_multiple_of(ALIGN) || len < MIN_HEAP {
        return false;
    }

    fl.base = base;
    fl.len = len;
    fl.head = NIL;
    unsafe {
        set(fl, 0, USED);
        set(fl, len - WORD, USED);
        set_block(fl, WORD, len - 2 * WORD, false);
        push(fl, WORD);
    }
    true
}

pub fn ralloc(fl: &mut FreeList, size: usize, align: usize) -> *mut u8 {
    if fl.base.is_null() || !align.is_power_of_two() {
        return null_mut();
    }
    let align = align.max(ALIGN);
    let Some(need) = block_for(size.max(1)) else {
        return null_mut();
    };

    unsafe {
        let mut h = fl.head;
        while h != NIL {
            let s = block_size(fl, h);
            let payload = h + WORD;
            let mut aligned = align_up(fl.base as usize + payload, align) - fl.base as usize;
            while aligned != payload && aligned - payload < MIN_BLOCK {
                aligned += align;
            }
            let gap = aligned - payload;

            if gap.checked_add(need).is_some_and(|total| total <= s) {
                return take(fl, h, s, gap, need);
            }
            h = next_free(fl, h);
        }
    }
    null_mut()
}

unsafe fn take(fl: &mut FreeList, h: usize, s: usize, gap: usize, need: usize) -> *mut u8 {
    unsafe {
        unlink(fl, h);

        let mut h = h;
        let mut s = s;
        if gap > 0 {
            set_block(fl, h, gap, false);
            push(fl, h);
            h += gap;
            s -= gap;
        }

        if s - need >= MIN_BLOCK {
            set_block(fl, h, need, true);
            set_block(fl, h + need, s - need, false);
            push(fl, h + need);
        } else {
            set_block(fl, h, s, true);
        }

        fl.base.add(h + WORD)
    }
}

// safety: ptr came from ralloc on this fl and hasn't been freed
pub unsafe fn free(fl: &mut FreeList, ptr: *mut u8) {
    unsafe { free_sized(fl, ptr, 0) }
}

// links landing in ptr..ptr + size go through ptr, the caller still has it borrowed (miri)
pub unsafe fn free_sized(fl: &mut FreeList, ptr: *mut u8, size: usize) {
    if ptr.is_null() {
        return;
    }
    unsafe {
        let payload = ptr.offset_from(fl.base) as usize;
        let mut h = payload - WORD;
        debug_assert!(h < fl.len && is_used(fl, h), "bad or double free");
        let mut s = block_size(fl, h);

        let prev_footer = get(fl, h - WORD);
        if prev_footer & USED == 0 {
            let ps = prev_footer & !USED;
            h -= ps;
            s += ps;
            unlink(fl, h);
        }

        let next = h + s;
        if !is_used(fl, next) {
            s += block_size(fl, next);
            unlink(fl, next);
        }

        set_block(fl, h, s, false);

        let old_head = fl.head;
        set_through(fl, h + WORD, old_head, ptr, payload, size);
        set_through(fl, h + 2 * WORD, NIL, ptr, payload, size);
        if old_head != NIL {
            set(fl, old_head + 2 * WORD, h);
        }
        fl.head = h;
    }
}

unsafe fn set_through(
    fl: &FreeList,
    off: usize,
    value: usize,
    ptr: *mut u8,
    start: usize,
    len: usize,
) {
    let bytes = value.to_ne_bytes();
    for i in 0..WORD {
        let at = off + i;
        unsafe {
            if at >= start && at < start + len {
                ptr.add(at - start).write(bytes[i]);
            } else {
                fl.base.add(at).write(bytes[i]);
            }
        }
    }
}

pub unsafe fn usable_size(fl: &FreeList, ptr: *mut u8) -> usize {
    unsafe {
        let h = ptr.offset_from(fl.base) as usize - WORD;
        block_size(fl, h) - OVERHEAD
    }
}

pub fn stats(fl: &FreeList) -> Stats {
    let mut st = Stats::default();
    if fl.base.is_null() {
        return st;
    }
    unsafe {
        let mut h = WORD;
        while h < fl.len - WORD {
            let s = block_size(fl, h);
            if is_used(fl, h) {
                st.used_blocks += 1;
                st.used_bytes += s;
            } else {
                st.free_blocks += 1;
                st.free_bytes += s;
                st.largest_free = st.largest_free.max(s);
            }
            h += s;
        }
    }
    st
}

pub fn check(fl: &FreeList) -> Result<(), String> {
    if fl.base.is_null() {
        return Ok(());
    }
    unsafe {
        if get(fl, 0) != USED || get(fl, fl.len - WORD) != USED {
            return Err("prologue or epilogue overwritten".to_string());
        }

        let mut h = WORD;
        let mut prev_was_free = false;
        let mut free_blocks = 0;
        while h < fl.len - WORD {
            let s = block_size(fl, h);
            if s < MIN_BLOCK || !s.is_multiple_of(ALIGN) || h + s > fl.len - WORD {
                return Err(format!("block at {h} has bad size {s}"));
            }
            if get(fl, h) != get(fl, h + s - WORD) {
                return Err(format!("block at {h} header and footer differ"));
            }
            let free = !is_used(fl, h);
            if free && prev_was_free {
                return Err(format!("block at {h} and the one before are both free"));
            }
            if free {
                free_blocks += 1;
            }
            prev_was_free = free;
            h += s;
        }
        if h != fl.len - WORD {
            return Err(format!("blocks end at {h}, not at the epilogue"));
        }

        let mut listed = 0;
        let mut prev = NIL;
        let mut f = fl.head;
        while f != NIL {
            if is_used(fl, f) {
                return Err(format!("used block at {f} is on the free list"));
            }
            if prev_free(fl, f) != prev {
                return Err(format!("free block at {f} has a bad prev link"));
            }
            listed += 1;
            if listed > free_blocks {
                return Err("free list has a cycle".to_string());
            }
            prev = f;
            f = next_free(fl, f);
        }
        if listed != free_blocks {
            return Err(format!(
                "{free_blocks} free blocks but {listed} on the list"
            ));
        }
    }
    Ok(())
}

pub fn dump(fl: &FreeList) {
    if fl.base.is_null() {
        return;
    }
    unsafe {
        let mut h = WORD;
        while h < fl.len - WORD {
            let s = block_size(fl, h);
            let state = if is_used(fl, h) { "used" } else { "free" };
            println!("{:p} {:>6} {}", fl.base.add(h + WORD), s, state);
            h += s;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Memory;

    fn heap<const N: usize>(mem: &mut Memory<N>) -> FreeList {
        let mut fl = FreeList::empty();
        assert!(unsafe { init(&mut fl, mem.0.as_mut_ptr(), N) });
        fl
    }

    #[test]
    fn one_free_block() {
        let mut mem = Memory([0; 1024]);
        let fl = heap(&mut mem);
        let st = stats(&fl);
        assert_eq!(st.free_blocks, 1);
        assert_eq!(st.free_bytes, 1024 - 2 * WORD);
        check(&fl).unwrap();
    }

    #[test]
    fn block_sizes() {
        assert_eq!(block_for(1), Some(MIN_BLOCK));
        assert_eq!(block_for(16), Some(32));
        assert_eq!(block_for(17), Some(48));
        assert_eq!(block_for(usize::MAX), None);
    }

    #[test]
    fn aligned_and_writable() {
        let mut mem = Memory([0; 4096]);
        let mut fl = heap(&mut mem);
        for align in [1, 2, 8, 16, 32, 64, 256] {
            let p = ralloc(&mut fl, 24, align);
            assert!(!p.is_null());
            assert_eq!(p as usize % align, 0);
            assert!(unsafe { usable_size(&fl, p) } >= 24);
            for i in 0..24 {
                unsafe { *p.add(i) = 0xAB };
            }
            check(&fl).unwrap();
        }
    }

    #[test]
    fn splits() {
        let mut mem = Memory([0; 1024]);
        let mut fl = heap(&mut mem);
        let p = ralloc(&mut fl, 16, 16);
        let st = stats(&fl);
        assert_eq!(st.used_blocks, 1);
        assert_eq!(st.used_bytes, 32);
        assert_eq!(st.free_blocks, 1);
        assert_eq!(st.free_bytes, 1024 - 16 - 32);
        unsafe { free(&mut fl, p) };
        check(&fl).unwrap();
    }

    #[test]
    fn no_split_when_remainder_too_small() {
        let mut mem = Memory([0; 64]);
        let mut fl = heap(&mut mem);
        let p = ralloc(&mut fl, 8, 16);
        assert!(!p.is_null());
        assert_eq!(unsafe { usable_size(&fl, p) }, 48 - OVERHEAD);
        assert_eq!(stats(&fl).free_blocks, 0);
        check(&fl).unwrap();
    }

    #[test]
    fn first_fit_reuses_freed_block() {
        let mut mem = Memory([0; 1024]);
        let mut fl = heap(&mut mem);
        let a = ralloc(&mut fl, 64, 16);
        let _b = ralloc(&mut fl, 64, 16);
        unsafe { free(&mut fl, a) };
        let c = ralloc(&mut fl, 32, 16);
        assert_eq!(a, c);
        check(&fl).unwrap();
    }

    #[test]
    fn coalesce_both_sides() {
        let mut mem = Memory([0; 1024]);
        let mut fl = heap(&mut mem);
        let a = ralloc(&mut fl, 32, 16);
        let b = ralloc(&mut fl, 32, 16);
        let c = ralloc(&mut fl, 32, 16);
        let d = ralloc(&mut fl, 32, 16);
        unsafe {
            free(&mut fl, a);
            free(&mut fl, c);
            check(&fl).unwrap();
            assert_eq!(stats(&fl).free_blocks, 3);
            free(&mut fl, b);
            check(&fl).unwrap();
            assert_eq!(stats(&fl).free_blocks, 2);
            free(&mut fl, d);
        }
        check(&fl).unwrap();
        let st = stats(&fl);
        assert_eq!(st.free_blocks, 1);
        assert_eq!(st.largest_free, 1024 - 2 * WORD);
    }

    #[test]
    fn out_of_memory() {
        let mut mem = Memory([0; 256]);
        let mut fl = heap(&mut mem);
        assert!(ralloc(&mut fl, 1024, 16).is_null());
        assert!(ralloc(&mut fl, usize::MAX, 16).is_null());
        assert!(ralloc(&mut fl, 8, 3).is_null());
        let p = ralloc(&mut fl, 200, 16);
        assert!(!p.is_null());
        assert!(ralloc(&mut fl, 64, 16).is_null());
        check(&fl).unwrap();
    }

    #[test]
    fn big_alignment_leaves_free_gap() {
        let mut mem = Memory([0; 8192]);
        let mut fl = heap(&mut mem);
        let p = ralloc(&mut fl, 100, 1024);
        assert_eq!(p as usize % 1024, 0);
        check(&fl).unwrap();
        unsafe { free(&mut fl, p) };
        check(&fl).unwrap();
        assert_eq!(stats(&fl).free_blocks, 1);
    }

    #[test]
    fn uninitialised_returns_null() {
        let mut fl = FreeList::empty();
        assert!(ralloc(&mut fl, 8, 8).is_null());
        let mut small = Memory([0; 32]);
        assert!(!unsafe { init(&mut fl, small.0.as_mut_ptr(), 32) });
    }

    // xorshift
    fn rand(state: &mut u64) -> u64 {
        *state ^= *state << 13;
        *state ^= *state >> 7;
        *state ^= *state << 17;
        *state
    }

    #[test]
    fn random_alloc_free() {
        let mut mem = Memory([0; 64 * 1024]);
        let mut fl = heap(&mut mem);
        let mut live: Vec<(*mut u8, usize, u8)> = Vec::new();
        let mut seed: u64 = 0x2545F4914F6CDD1D;
        let rounds = if cfg!(miri) { 300 } else { 5000 };

        for round in 0..rounds {
            let r = rand(&mut seed);
            if !r.is_multiple_of(3) || live.is_empty() {
                let size = (rand(&mut seed) % 700) as usize + 1;
                let align = 1 << (rand(&mut seed) % 8);
                let p = ralloc(&mut fl, size, align);
                if p.is_null() {
                    continue;
                }
                assert_eq!(p as usize % align, 0);
                let fill = round as u8;
                for i in 0..size {
                    unsafe { *p.add(i) = fill };
                }
                live.push((p, size, fill));
            } else {
                let i = (rand(&mut seed) as usize) % live.len();
                let (p, size, fill) = live.swap_remove(i);
                for j in 0..size {
                    assert_eq!(unsafe { *p.add(j) }, fill, "allocation was overwritten");
                }
                unsafe { free(&mut fl, p) };
            }
            if round % 50 == 0 {
                check(&fl).unwrap();
            }
        }

        for (p, _, _) in live {
            unsafe { free(&mut fl, p) };
        }
        check(&fl).unwrap();
        assert_eq!(stats(&fl).free_blocks, 1);
    }
}

#![allow(clippy::needless_range_loop)]
#![allow(clippy::missing_safety_doc)]

pub mod bump;
pub mod freelist;
pub mod global;

#[repr(C, align(16))]
pub struct Memory<const N: usize>(pub [u8; N]);

pub const fn align_up(x: usize, align: usize) -> usize {
    (x + align - 1) & !(align - 1)
}

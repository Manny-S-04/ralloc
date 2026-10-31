use ralloc::Memory;
use ralloc::freelist::{FreeList, dump, free, init, ralloc};

fn main() {
    let mut mem = Memory([0; 1024]);
    let mut fl = FreeList::empty();
    unsafe { init(&mut fl, mem.0.as_mut_ptr(), 1024) };

    let a = ralloc(&mut fl, 16, 16);
    let b = ralloc(&mut fl, 100, 64);
    let c = ralloc(&mut fl, 40, 8);
    for i in 0..16 {
        unsafe { *a.add(i) = b'A' + i as u8 };
    }
    println!("a, b, c allocated");
    dump(&fl);

    unsafe { free(&mut fl, b) };
    println!("\nb freed");
    dump(&fl);

    unsafe {
        free(&mut fl, a);
        free(&mut fl, c);
    }
    println!("\nall freed, back to one block");
    dump(&fl);
}

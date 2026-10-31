use ralloc::global::Ralloc;
use std::collections::HashMap;
use std::thread;

#[global_allocator]
static GLOBAL: Ralloc<{ 16 * 1024 * 1024 }> = Ralloc::new();

#[test]
fn vec_string_hashmap() {
    let mut v: Vec<u64> = Vec::new();
    for i in 0..10_000 {
        v.push(i);
    }
    assert_eq!(v.iter().sum::<u64>(), 10_000 * 9_999 / 2);

    let mut s = String::new();
    for i in 0..1000 {
        s.push_str(&i.to_string());
    }
    assert!(s.starts_with("0123456789101112"));

    let mut m: HashMap<String, usize> = HashMap::new();
    for i in 0..2000 {
        m.insert(format!("key{i}"), i);
    }
    assert_eq!(m["key1234"], 1234);
    GLOBAL.check().unwrap();
}

#[test]
fn threads() {
    let mut handles = Vec::new();
    for t in 0..8 {
        handles.push(thread::spawn(move || {
            let mut total = 0;
            for i in 0..500 {
                let v: Vec<usize> = (0..(i % 64) + t).collect();
                total += v.len();
            }
            total
        }));
    }
    for h in handles {
        assert!(h.join().unwrap() > 0);
    }
    GLOBAL.check().unwrap();
}

#[test]
fn big_alignment() {
    #[repr(align(4096))]
    struct Page([u8; 4096]);
    let pages: Vec<Box<Page>> = (0..4).map(|_| Box::new(Page([1; 4096]))).collect();
    for p in &pages {
        assert_eq!(&**p as *const Page as usize % 4096, 0);
        assert_eq!(p.0[4095], 1);
    }
    GLOBAL.check().unwrap();
}

#[test]
fn memory_comes_back() {
    let before = GLOBAL.stats().used_bytes;
    {
        let v: Vec<Vec<u8>> = (0..100).map(|i| vec![0u8; i * 10]).collect();
        assert_eq!(v.len(), 100);
    }
    // other tests run in parallel
    assert!(GLOBAL.stats().used_bytes < before + 1024 * 1024);
}

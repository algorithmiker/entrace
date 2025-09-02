fn main() {
    divan::main();
}
use bitvec::vec::BitVec;
use divan::{Bencher, black_box};
use rand::Rng;
const KB: usize = 1024;
const MB: usize = 1024 * 1024;
const _10MIL: usize = 10_000_000;
#[divan::bench(args = [KB,MB,_10MIL,])]
fn push_true_vec(n: usize) {
    let mut v = vec![];
    #[allow(clippy::same_item_push)]
    for _ in 0..n {
        v.push(true);
    }
}
#[divan::bench(args = [KB,MB,_10MIL,])]
fn push_true_bitvec(n: usize) {
    let mut v: BitVec<usize> = BitVec::new();
    for _ in 0..n {
        v.push(true);
    }
}

#[divan::bench(args = [KB,MB,_10MIL,])]
fn repeat_vec(n: usize) {
    let _v = vec![true; n];
}
#[divan::bench(args = [KB,MB,_10MIL,])]
fn repeat_bitvec(n: usize) {
    let _v: BitVec<u64> = BitVec::repeat(true, n);
}

#[divan::bench(args = [KB,MB,_10MIL,])]
fn random_access_vec(bencher: Bencher, n: usize) {
    let mut rng = rand::rng();
    let mut vec = Vec::with_capacity(n);
    for _i in 0..n {
        vec.push(rng.random::<bool>());
    }
    let indices: Vec<usize> = (0..(n / 100)).map(|_| rng.random_range(0..n)).collect();
    bencher.bench(move || {
        black_box(|| {
            for index in &indices {
                vec.get(*index).unwrap();
            }
        })();
    });
}
#[divan::bench(args = [KB,MB,_10MIL,])]
fn random_access_bitvec(bencher: Bencher, n: usize) {
    let mut rng = rand::rng();
    let mut vec: BitVec<usize> = BitVec::with_capacity(n);
    for _i in 0..n {
        vec.push(rng.random::<bool>());
    }
    let indices: Vec<usize> = (0..(n / 100)).map(|_| rng.random_range(0..n)).collect();
    bencher.bench(move || {
        black_box(|| {
            for index in &indices {
                vec.get(*index).unwrap();
            }
        })();
    });
}

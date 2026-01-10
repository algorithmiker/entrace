use entrace_core::{TreeLayer, mmap::ETStorage};
use std::io::Cursor;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::thread;
use tracing::info_span;
use tracing_subscriber::prelude::*;

// Test to reproduce issue 4. This is only interesting on Windows.
#[test]
fn test_issue_4() {
    type VecCursor = Cursor<Vec<u8>>;
    let storage = Arc::new(ETStorage::<VecCursor, VecCursor>::init(Cursor::new(vec![])));
    tracing_subscriber::registry().with(TreeLayer::from_storage(storage.clone())).init();
    let running = Arc::new(AtomicBool::new(true));
    let mut handles = vec![];

    for _ in 0..8 {
        let running = running.clone();
        handles.push(thread::spawn(move || {
            while running.load(Ordering::Relaxed) {
                let parent = Arc::new(info_span!("parent"));
                let p1 = parent.clone();
                let p2 = parent.clone();

                let t1 = thread::spawn(move || {
                    // ideally this gets a lower id but sends *later*
                    let _c1 = info_span!(parent: &*p1, "C1");
                });

                let t2 = thread::spawn(move || {
                    // ideally this gets a higher id but sends *sooner*
                    let c2 = info_span!(parent: &*p2, "C2");
                    // a grandchild, because why not.
                    let _gc = info_span!(parent: &c2, "GC");
                });

                t1.join().unwrap();
                t2.join().unwrap();
            }
        }));
    }
    std::thread::sleep(std::time::Duration::from_secs(5));
    running.store(false, Ordering::Relaxed);
    for h in handles {
        h.join().unwrap();
    }
}

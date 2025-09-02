use std::{io::Cursor, sync::Arc};

use entrace_core::{TreeLayer, mmap::ETStorage};
use tracing::{info, level_filters::LevelFilter};
use tracing_subscriber::{Registry, layer::SubscriberExt, util::SubscriberInitExt};

fn get_hello_et() -> Vec<u8> {
    let buf = Cursor::new(vec![]);
    let storage = Arc::new(ETStorage::init(buf));
    let tree_layer = TreeLayer::from_storage(storage.clone());
    Registry::default().with(LevelFilter::TRACE).with(tree_layer).init();
    info!("h");
    let tmp_buf = Cursor::new(vec![]);
    let finish_val = storage.finish(tmp_buf).unwrap();

    let out_buf = finish_val.temp_buf.unwrap();
    out_buf.into_inner()
}

#[test]
fn test_et_iet_et() {
    let hello_ht = get_hello_et();
    println!("hello_ht = {hello_ht:?}, len={}", hello_ht.len());
    let hello_ht_orig = hello_ht.clone();

    let mut c1_in = Cursor::new(hello_ht);
    let mut c1_out = Cursor::new(vec![]);
    entrace_core::convert::et_to_iet(&mut c1_in, &mut c1_out, true).unwrap();

    let hello_iht = c1_out.into_inner();
    println!("hello_iht = {hello_iht:?}, len={}", hello_iht.len());

    let mut c2_in = Cursor::new(hello_iht);
    let mut c2_out = Cursor::new(vec![]);
    entrace_core::convert::iet_to_et(&mut c2_in, &mut c2_out, true, false).unwrap();

    pretty_assertions::assert_eq!(hello_ht_orig, c2_out.into_inner());
}

use std::{
    io::{Cursor, Seek},
    sync::Arc,
};

use entrace_core::{
    TreeLayer,
    remote::{IETStorage, IETStorageConfig},
};
use tracing::{info, level_filters::LevelFilter};
use tracing_subscriber::{Registry, layer::SubscriberExt, util::SubscriberInitExt};

fn get_hello_iet() -> Vec<u8> {
    let buf = vec![];
    let storage = Arc::new(IETStorage::init(IETStorageConfig::non_length_prefixed(buf)));
    let tree_layer = TreeLayer::from_storage(storage.clone());
    Registry::default().with(LevelFilter::TRACE).with(tree_layer).init();
    info!("h");

    storage.finish().unwrap()
}

#[test]
fn test_iet_et_iet() {
    let hello_iht = get_hello_iet();
    let hello_iht_orig = hello_iht.clone();

    let mut c1_in = Cursor::new(hello_iht);
    let mut c1_out = Cursor::new(vec![]);
    entrace_core::convert::iet_to_et(&mut c1_in, &mut c1_out, true, false).unwrap();

    c1_out.rewind().unwrap();
    let mut c2_out = Cursor::new(vec![]);
    entrace_core::convert::et_to_iet(&mut c1_out, &mut c2_out, true).unwrap();
    let hello_iht = c2_out.into_inner();

    pretty_assertions::assert_eq!(hello_iht_orig, hello_iht);
}

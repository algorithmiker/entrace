use std::{fs::File, io::BufWriter, sync::Arc};

use entrace_core::{
    TreeLayer,
    remote::{IETStorage, IETStorageConfig},
};
use tracing::{info, level_filters::LevelFilter};
use tracing_subscriber::{Registry, layer::SubscriberExt, util::SubscriberInitExt};

fn main() {
    let file = BufWriter::new(File::create("hello.iet").unwrap());
    let storage = Arc::new(IETStorage::init(IETStorageConfig::non_length_prefixed(file)));
    let tree_layer = TreeLayer::from_storage(storage.clone());
    Registry::default().with(LevelFilter::TRACE).with(tree_layer).init();

    info!(target = "World", "Hello");

    storage.finish().unwrap();
}

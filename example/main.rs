use entrace_core::IETBuilder;
use tracing::{info, level_filters::LevelFilter};
use tracing_subscriber::{Registry, layer::SubscriberExt, util::SubscriberInitExt};

fn main() {
    let (layer, _guard) = IETBuilder::from_file("hello.iet").unwrap().build().unwrap();
    Registry::default().with(LevelFilter::TRACE).with(layer).init();

    info!(target = "World", "Hello");
}

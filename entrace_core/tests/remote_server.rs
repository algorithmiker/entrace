use std::time::{Duration, Instant};

use entrace::{IETBuilder, IETPresentationConfig, LogProvider, remote::RemoteLogProvider};
use tracing::{info, level_filters::LevelFilter};
use tracing_subscriber::{Registry, layer::SubscriberExt, util::SubscriberInitExt};

#[test]
fn remote_server() {
    let builder = IETBuilder::serve("localhost:0").unwrap();
    let addr = builder.listener.local_addr().unwrap();
    println!("Started server on {addr}");
    let (layer, guard) = builder.build().unwrap();

    Registry::default().with(LevelFilter::TRACE).with(layer).init();
    info!("Hello world");
    let mut client = RemoteLogProvider::connect(addr, IETPresentationConfig::default());
    drop(guard);
    let start = Instant::now();
    while start.elapsed() < Duration::from_millis(100) {
        client.run_event_loop();
    }

    pretty_assertions::assert_eq!(client.len(), 2);
    pretty_assertions::assert_eq!(
        client.message(1).expect("Can't get message for 1").unwrap(),
        "Hello world"
    );
}

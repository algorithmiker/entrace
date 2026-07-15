use std::{
    net::TcpListener,
    thread::sleep,
    time::{Duration, Instant},
};

use entrace::{IETBuilder, IETPresentationConfig, LogProvider, remote::RemoteLogProvider};
use tracing::{info, level_filters::LevelFilter};
use tracing_subscriber::{Registry, layer::SubscriberExt, util::SubscriberInitExt};

#[test]
fn remote_client() {
    let listener = TcpListener::bind("localhost:0").unwrap();
    let addr = listener.local_addr().unwrap();
    println!("Started server on {addr}");
    let mut server = RemoteLogProvider::new(listener, IETPresentationConfig::default());
    sleep(Duration::from_millis(100));

    let builder = IETBuilder::connect(addr).unwrap();
    let (layer, guard) = builder.build().unwrap();
    Registry::default().with(LevelFilter::TRACE).with(layer).init();
    info!("Hello world");
    drop(guard);

    let start = Instant::now();
    while start.elapsed() < Duration::from_millis(100) {
        server.run_event_loop();
    }

    pretty_assertions::assert_eq!(server.len(), 2);
    pretty_assertions::assert_eq!(
        server.message(1).expect("Can't get message for 1").unwrap(),
        "Hello world"
    );
}

use clap::{Parser, ValueEnum};
use entrace_core::{
    mmap::ETStorage,
    remote::{IETStorage, IETStorageConfig},
    TreeLayer,
};
use petgraph::{
    graph::{DiGraph, NodeIndex},
    Direction,
};
use std::{
    fmt::Display,
    fs::OpenOptions,
    net::TcpStream,
    sync::Arc,
    thread::{self, sleep},
    time::{Duration, Instant},
};
use tracing::{debug, info, info_span, instrument, level_filters::LevelFilter, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, Registry};

#[instrument(
    skip(graph),
    fields(
        node_id = ?root,
        node_value = %graph[root]
    )
)]
fn dfs<T: Display + std::fmt::Debug, E>(graph: &DiGraph<T, E>, root: NodeIndex) {
    info!("visiting node");

    for neighbor in graph.neighbors_directed(root, Direction::Outgoing) {
        dfs(graph, neighbor);
    }
}

fn small_tree() {
    info!("Starting graph traversal");
    let mut graph = DiGraph::new();
    let a = graph.add_node('a');
    let b = graph.add_node('b');
    let c = graph.add_node('c');
    let d = graph.add_node('d');
    let e = graph.add_node('e');
    let f = graph.add_node('f');

    graph.add_edge(a, b, ());
    graph.add_edge(a, c, ());
    graph.add_edge(b, d, ());
    graph.add_edge(b, e, ());
    graph.add_edge(d, f, ());

    dfs(&graph, a);

    info!("Finished graph traversal");
}
fn very_simple_graph() {
    info!("Starting graph traversal");
    let mut graph = DiGraph::new();
    let a = graph.add_node('a');
    let b = graph.add_node('b');
    let c = graph.add_node('c');

    graph.add_edge(a, b, ());
    graph.add_edge(a, c, ());
    dfs(&graph, a);

    info!("Finished graph traversal");
}
fn small_tree_threads() {
    info!("Starting graph traversal");
    let handle = thread::spawn(|| {
        info_span!("threaded").in_scope(|| {
            info!("Inner thread message 1");
            sleep(Duration::from_micros(10));
            info!("Inner thread message 2");
        });
    });

    let mut graph = DiGraph::new();
    let a = graph.add_node('a');
    let b = graph.add_node('b');
    let c = graph.add_node('c');
    let d = graph.add_node('d');
    let e = graph.add_node('e');
    let f = graph.add_node('f');

    graph.add_edge(a, b, ());
    graph.add_edge(a, c, ());
    graph.add_edge(b, d, ());
    graph.add_edge(b, e, ());
    graph.add_edge(d, f, ());

    dfs(&graph, a);

    info!("Finished graph traversal");
    handle.join().unwrap();
}
fn paramtree(depth: usize, breadth: usize) {
    info!("Starting graph traversal");
    let mut graph = DiGraph::new();
    let root = graph.add_node("root".into());
    let mut last_level = vec![root; breadth];
    for j in 0..depth {
        let mut new_level = vec![];
        #[allow(clippy::needless_range_loop)]
        for i in 0..breadth {
            let new_node = graph.add_node(format!("d{j}b{i}"));
            graph.add_edge(last_level[i], new_node, ());
            new_level.push(new_node);
        }
        last_level = new_level;
    }
    println!("Constructed tree");
    dfs(&graph, root);

    info!("Finished graph traversal");
}
pub fn spammer(n: usize) {
    for i in 0..n {
        info!(msg_idx = i, "Message {i}");
        if i % 1024 == 0 {
            println!("Spammer at message {i}");
        }
    }
}
pub fn spammer_newline(n: usize) {
    for i in 0..n {
        info!(attr = "Multi\nLine Attribute", "Message {i}\nSecond line");
        if i % 1024 == 0 {
            println!("Spammer at message {i}");
        }
    }
}
pub fn bursts(spam: usize, sleep_time: Duration) {
    let mut burst_no = 0;
    let mut total = 0;
    loop {
        for i in 0..spam {
            info!(total, local = i, burst_no, "Message {total}");
            total += 1;
        }
        burst_no += 1;
        warn!(burst_no, len = spam, "Burst done");
        sleep(sleep_time);
    }
}
pub fn timer() {
    for i in 1.. {
        info!("Message {i}");
        println!("timer Message {i}");
        sleep(Duration::from_secs(1));
    }
}
pub fn colors() {
    tracing::trace!("Trace");
    debug!("Debug");
    info!("Info");
    warn!("Warn");
    tracing::error!("Error");
}
pub fn work(args: &Args) {
    match args.work {
        Work::SmallTree => small_tree(),
        Work::SmallTreeThreads => small_tree_threads(),
        Work::VerySimpleGraph => very_simple_graph(),
        Work::Paramtree => paramtree(1000, 5_000),
        Work::MultiLine => spammer_newline(1000),
        Work::Spammer => spammer(1_000_000),
        Work::Bursts => bursts(10_000, Duration::from_secs(1)),
        Work::Timer => timer(),
        Work::Colors => colors(),
        Work::HelloWorld => {
            info!("Hello")
        }
    }
    //paramtree(1000, 10_00); // about 250 mb
    //paramtree(1000, 50_00); // about 1.3 GB
    //paramtree(1000, 10_000); // about 2.5 GB
    //infinite();
    // spammer(10_000_000);
}
pub fn time<T>(a: impl FnOnce() -> T) -> (Duration, T) {
    let start = Instant::now();
    let res = a();
    (start.elapsed(), res)
}
#[derive(ValueEnum, Debug, Clone)]
pub enum Work {
    SmallTree,
    SmallTreeThreads,
    VerySimpleGraph,
    Paramtree,
    Spammer,
    Timer,
    Bursts,
    Colors,
    HelloWorld,
    MultiLine,
}
#[derive(ValueEnum, Debug, Clone, Default)]
pub enum LogMode {
    #[default]
    DiskET,
    DiskIET,
    StreamingET,
}
#[derive(Parser)]
#[command(name = "entrace_graph_example")]
pub struct Args {
    #[arg(short = 'f', long)]
    pub log_file: Option<String>,
    #[arg(short = 'm', long)]
    pub log_mode: LogMode,
    pub work: Work,
}

fn setup_tracing(args: &Args) -> Box<dyn FnOnce(&Args)> {
    let log_filename = match args.log_file.as_ref() {
        Some(x) => x.as_str(),
        None => match args.log_mode {
            LogMode::DiskET => "log.et",
            LogMode::DiskIET => "log.iet",
            LogMode::StreamingET => "localhost:8000",
        },
    };
    pub fn getf(filename: &str) -> std::io::Result<std::fs::File> {
        OpenOptions::new().truncate(true).create(true).write(true).read(true).open(filename)
    }

    match args.log_mode {
        LogMode::DiskET => {
            let file = getf(log_filename).unwrap();
            let storage = Arc::new(ETStorage::init(file));
            let tree_layer = TreeLayer::from_storage(storage.clone());
            Registry::default().with(LevelFilter::TRACE).with(tree_layer).init();
            let l_fn2 = log_filename.to_string();
            Box::new(move |_args| {
                let temporary_file = OpenOptions::new()
                    .write(true)
                    .create(true)
                    .read(true)
                    .truncate(true)
                    .open("entrace.log.tmp")
                    .unwrap();

                storage.finish(temporary_file).unwrap();
                std::fs::rename("entrace.log.tmp", l_fn2).unwrap();
            })
        }
        LogMode::DiskIET => {
            let file = getf(log_filename).unwrap();
            let storage = Arc::new(IETStorage::init(IETStorageConfig::non_length_prefixed(file)));
            let tree_layer = TreeLayer::from_storage(storage.clone());
            Registry::default().with(LevelFilter::TRACE).with(tree_layer).init();
            Box::new(move |_args| {
                storage.finish().unwrap();
            })
        }
        LogMode::StreamingET => {
            let tcp_stream = TcpStream::connect(log_filename).unwrap();
            let storage = Arc::new(IETStorage::init(IETStorageConfig::length_prefixed(tcp_stream)));
            let tree_layer = TreeLayer::from_storage(storage.clone());
            Registry::default().with(LevelFilter::TRACE).with(tree_layer).init();
            Box::new(move |_args| {
                storage.finish().unwrap();
            })
        }
    }
}
pub fn time_print<T>(tag: &str, f: impl FnOnce() -> T) -> T {
    let timed = time(f);
    println!("{tag} took {:?}", timed.0);
    timed.1
}
fn main() {
    let args = time_print("parsing args", Args::parse);

    //let file_appender = tracing_appender::rolling::daily("./logs", "graph_trace.log.json");
    //let (non_blocking_writer, _guard) = tracing_appender::non_blocking(file_appender);

    //let json_layer = tracing_subscriber::fmt::layer()
    //    .json()
    //    .with_span_list(true)
    //    .with_file(true)
    //    .with_line_number(true)
    //    .with_writer(non_blocking_writer);

    let finish_callback = setup_tracing(&args);
    let start = std::time::Instant::now();
    work(&args);
    println!("Work, WITH tracing, took {:?}", start.elapsed());
    finish_callback(&args);
}

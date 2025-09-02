 ENTRACE is a modern **log viewer and observability toolkit for Rust** built on the excellent [tracing](https://crates.io/crates/tracing) crate.

 It facilitates better viewing, exploring, and storage for logs for small-to-medium sized applications, where other solutions (OpenTelemetry, Grafana/Loki, ...) are overkill.

 The ENTRACE toolkit consists of:

   - **file formats** for storage of the structured data associated with traces
   - a **client library** that provides writers and readers for the file formats, conversion between them, and remote tracing over TCP
   - a **graphical log viewer** built with [egui](https://github.com/emilk/egui)
   - a **Lua API** for querying span information, which is used for performing queries on structured data in the GUI
   - a **formatter** for the [tracing_subscriber] crate, which formats events printed to the console in a less verbose manner.

 ENTRACE is provided at no cost and without warranty.

 ## Adding ENTRACE to your library
 To start recording traces with entrace, you first need to add [tracing_subscriber] to your
 dependencies.

 `entrace_core` provides a [TreeLayer], which is a [tracing_subscriber::Layer].

 ### Producing IET files
 ```rust,ignore
 use entrace_core::{TreeLayer, remote::IETStorage, remote::IETStorageConfig};
 use std::{sync::Arc, fs::OpenOptions};
 use tracing::{info, level_filters::LevelFilter};
 use tracing_subscriber::{Registry, layer::SubscriberExt, util::SubscriberInitExt};

 let file = OpenOptions::new()
     .write(true)
     .create(true)
     .open("my_program.iet")
     .unwrap();
 let storage = Arc::new(IETStorage::init(IETStorageConfig::non_length_prefixed(file)));
 let tree_layer = TreeLayer::from_storage(storage.clone());
 Registry::default().with(LevelFilter::TRACE).with(tree_layer).init();
 info!(target = "World", "Hello");

 // ...

 // save the trace when you shut down the process
 storage.finish();
 ```

 ### Producing ET files
 ```rust,ignore
 use entrace_core::{TreeLayer, mmap::ETStorage};
 use std::{sync::Arc, fs::OpenOptions};
 use tracing::{info, level_filters::LevelFilter};
 use tracing_subscriber::{Registry, layer::SubscriberExt, util::SubscriberInitExt};

 let file = OpenOptions::new()
     .write(true)
     .create(true)
     .truncate(true)
     .open("my_program.et")
     .unwrap();
 let storage = Arc::new(ETStorage::init(file));
 let tree_layer = TreeLayer::from_storage(storage.clone());
 Registry::default().with(LevelFilter::TRACE).with(tree_layer).init();
 info!(target = "World", "Hello");

 // ...

 // ETStorage initially writes an appendable .iet file, then converts it to .et and writes it to
 // the provided temporary file.
 // Here we perform an atomic swap of the two files, but you could also utilize an in-memory
 // buffer for this.
 let temp_file = OpenOptions::new()
     .write(true)
     .create(true)
     .truncate(true)
     .open("my_program.tmp")
     .unwrap();
 storage.finish(temp_file);
 std::fs::rename("my_program.tmp", "my_program.et").unwrap();
 ```

 ### Remote tracing
 To perform remote tracing, you just need to give a [std::net::TcpStream] to [remote::IETStorage].
 ```rust,ignore
 use entrace_core::{TreeLayer, remote::IETStorage, remote::IETStorageConfig};
 use std::{sync::Arc, fs::OpenOptions, net::TcpStream};
 use tracing::{info, level_filters::LevelFilter};
 use tracing_subscriber::{Registry, layer::SubscriberExt, util::SubscriberInitExt};

 let tcp_stream = TcpStream::connect("localhost:3000").unwrap();
 let storage = Arc::new(IETStorage::init(IETStorageConfig::length_prefixed(tcp_stream)));
 let tree_layer = TreeLayer::from_storage(storage.clone());
 Registry::default().with(LevelFilter::TRACE).with(tree_layer).init();
 info!(target = "World", "Hello");

 // ...

 // save the trace when you shut down the process
 storage.finish();
 ```

## Reading traces
ENTRACE provides the [LogProvider] interface for reading the data contained in a trace.
- To read any type of trace from a file, use [load_trace].
- To set up a remote server, use [crate::remote::RemoteLogProvider::new]

## Converting traces
The [crate::convert] module provides several functions for converting between ET and IET files, and vice versa.

## Querying
Currently, the query system of the ENTRACE GUI is quite tied to the graphical interface itself.

You might want to import just the query module from the GUI, or vendor it into your project.
Alternatively, since most of the methods provided by the GUI are only thin wrappers over the functions provided by [LogProvider], you can very easily write your own, better query system as well.

## [`tracing_subscriber`] formatter
A nicer formatter for `tracing_subscriber` is included in [crate::en_formatter].
Usage:

```rust
use entrace_core::en_formatter::EnFormatter;
use tracing::{level_filters::LevelFilter};
use tracing_subscriber::{Registry, layer::SubscriberExt, util::SubscriberInitExt};
let printing_layer =
    tracing_subscriber::fmt::layer().without_time().event_format(EnFormatter);

// add more .with() layers if you want to 
Registry::default().with(LevelFilter::TRACE).with(printing_layer).init();
```

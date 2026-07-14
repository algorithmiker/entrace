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

`entrace` provides a [TreeLayer], which is a [tracing_subscriber::Layer].

### Producing IET files
 ```rust
use entrace::IETBuilder;
use tracing::{info, level_filters::LevelFilter};
use tracing_subscriber::{Registry, layer::SubscriberExt, util::SubscriberInitExt};

fn main() {
    let (layer, _guard) = IETBuilder::from_file("hello.iet").unwrap().build().unwrap();
    Registry::default().with(LevelFilter::TRACE).with(layer).init();

    info!(target = "World", "Hello");
}
 ```

### Producing ET files
 ```rust
use entrace::ETBuilder;
use tracing::{info, level_filters::LevelFilter};
use tracing_subscriber::{Registry, layer::SubscriberExt, util::SubscriberInitExt};

fn main() {
    let (layer, _guard) = ETBuilder::from_file("hello.et").unwrap().build().unwrap();
    Registry::default().with(LevelFilter::TRACE).with(layer).init();

    info!(target = "World", "Hello");
}
 ```

### Remote tracing
 ```rust,ignore
use entrace::IETBuilder;
use tracing::{info, level_filters::LevelFilter};
use tracing_subscriber::{Registry, layer::SubscriberExt, util::SubscriberInitExt};

fn main() {
    let (layer, _guard) = IETBuilder::connect("localhost:8080").unwrap().build().unwrap();
    Registry::default().with(LevelFilter::TRACE).with(layer).init();

    info!(target = "World", "Hello");
}
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
use entrace::en_formatter::EnFormatter;
use tracing::{level_filters::LevelFilter};
use tracing_subscriber::{Registry, layer::SubscriberExt, util::SubscriberInitExt};
let printing_layer =
    tracing_subscriber::fmt::layer().without_time().event_format(EnFormatter);

// add more .with() layers if you want to 
Registry::default().with(LevelFilter::TRACE).with(printing_layer).init();
```

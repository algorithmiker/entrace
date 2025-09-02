//! A better formatter for [tracing_subscriber].

use std::fmt;

use tracing::{Event, Subscriber};
use tracing_subscriber::{
    fmt::{FormatEvent, FormatFields, format::Writer},
    registry::LookupSpan,
};

pub struct EnFormatter;
const GREY: &str = "\x1b[90m";
const GREEN: &str = "\x1b[32m";
const BLUE: &str = "\x1b[34m";
const YELLOW: &str = "\x1b[33m";
const RED: &str = "\x1b[31m";
const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const RESET_BOLD: &str = "\x1b[22m";
impl<S, N> FormatEvent<S, N> for EnFormatter
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    N: for<'a> FormatFields<'a> + 'static,
{
    fn format_event(
        &self, ctx: &tracing_subscriber::fmt::FmtContext<'_, S, N>, mut writer: Writer<'_>,
        event: &Event<'_>,
    ) -> fmt::Result {
        let metadata = event.metadata();

        let (level, color) = match *metadata.level() {
            tracing::Level::TRACE => ("T", GREY),
            tracing::Level::DEBUG => ("D", GREEN),
            tracing::Level::INFO => ("I", BLUE),
            tracing::Level::WARN => ("W", YELLOW),
            tracing::Level::ERROR => ("E", RED),
        };
        write!(writer, "{BOLD}{color}[{level}]{RESET_BOLD} {}:{RESET} ", metadata.target())?;
        ctx.field_format().format_fields(writer.by_ref(), event)?;
        writeln!(writer)
    }
}

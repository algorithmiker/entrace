use std::{
    fmt::Display,
    fs::OpenOptions,
    path::PathBuf,
    process,
    sync::{Arc, RwLock},
};

use anyhow::Context;
use directories::ProjectDirs;
use entrace_core::{
    TreeLayer,
    en_formatter::EnFormatter,
    remote::{IETStorage, IETStorageConfig},
};
use tracing::{info, level_filters::LevelFilter};
use tracing_subscriber::{Registry, layer::SubscriberExt, util::SubscriberInitExt};

use crate::spawn_task;
#[derive(Debug, Clone, PartialEq)]
pub enum SelfTracingLevel {
    Disabled,
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}
impl SelfTracingLevel {
    pub fn to_filter(&self) -> LevelFilter {
        match self {
            SelfTracingLevel::Disabled => LevelFilter::OFF,
            SelfTracingLevel::Trace => LevelFilter::TRACE,
            SelfTracingLevel::Debug => LevelFilter::DEBUG,
            SelfTracingLevel::Info => LevelFilter::INFO,
            SelfTracingLevel::Warn => LevelFilter::WARN,
            SelfTracingLevel::Error => LevelFilter::ERROR,
        }
    }
}
impl SelfTracingLevel {
    pub fn repr_first_up(&self) -> &'static str {
        match self {
            SelfTracingLevel::Disabled => "Disabled",
            SelfTracingLevel::Trace => "Trace",
            SelfTracingLevel::Debug => "Debug",
            SelfTracingLevel::Info => "Info",
            SelfTracingLevel::Warn => "Warn",
            SelfTracingLevel::Error => "Error",
        }
    }
    pub fn repr_first_low(&self) -> &'static str {
        match self {
            SelfTracingLevel::Disabled => "disabled",
            SelfTracingLevel::Trace => "trace",
            SelfTracingLevel::Debug => "debug",
            SelfTracingLevel::Info => "info",
            SelfTracingLevel::Warn => "warn",
            SelfTracingLevel::Error => "error",
        }
    }
}
impl Display for SelfTracingLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.repr_first_up())
    }
}
#[derive(Default)]
pub enum SelfTracingState {
    #[default]
    Disabled,
    Enabled(SelfTracingInner),
}
pub fn self_tracing_path() -> anyhow::Result<PathBuf> {
    let log_dir = ProjectDirs::from("org", "entrace", "entrace")
        .context("Cannot get base dir for self-logging")?;
    let log_dir = log_dir.data_dir();
    std::fs::create_dir_all(log_dir)
        .with_context(|| format!("Failed to create base dir at {log_dir:?} for self logging"))?;
    let pid = process::id();
    let mut log_path = log_dir.to_path_buf();
    log_path.push(format!("log_{pid}.iet"));
    Ok(log_path)
}

impl SelfTracingState {
    pub fn start_tracing(level: SelfTracingLevel, save_trace: bool) -> SelfTracingState {
        let filter_level = level.to_filter();
        let st_path = Arc::new(RwLock::new(None));
        let path_c = st_path.clone();
        spawn_task(move || {
            let printing_layer =
                tracing_subscriber::fmt::layer().without_time().event_format(EnFormatter);
            if save_trace {
                let path = self_tracing_path().unwrap();
                let file = OpenOptions::new()
                    .write(true)
                    .create(true)
                    .truncate(true)
                    .open(&path)
                    .with_context(|| {
                        format!(
                            "Cannot open log file at {} for self-logging",
                            path.to_string_lossy()
                        )
                    })
                    .unwrap();
                let storage =
                    Arc::new(IETStorage::init(IETStorageConfig::non_length_prefixed(file)));
                let tree_layer = TreeLayer::from_storage(storage);
                Registry::default().with(filter_level).with(printing_layer).with(tree_layer).init();
                info!(path = path.display().to_string(), "path for self_tracing");
                *st_path.write().unwrap() = Some(path.display().to_string());
            } else {
                info!("Not saving self-trace file");
                Registry::default().with(filter_level).with(printing_layer).init();
            }
            info!("Started self-tracing");
        });
        SelfTracingState::Enabled(SelfTracingInner { level, path: path_c, saving: save_trace })
    }
}
pub struct SelfTracingInner {
    pub level: SelfTracingLevel,
    pub saving: bool,
    pub path: Arc<RwLock<Option<String>>>,
}

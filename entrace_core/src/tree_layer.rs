use std::{
    collections::HashMap,
    sync::{Arc, RwLock, atomic::AtomicU32},
};

use tracing::{Subscriber, error};
use tracing_subscriber::Layer;

use crate::Storage;

pub struct TreeLayer<S: Storage> {
    pub id_to_pool: RwLock<HashMap<tracing::span::Id, u32>>,
    pub counter: AtomicU32,
    pub storage: Arc<S>,
}
impl<S: Storage> TreeLayer<S> {
    pub fn from_storage(storage: Arc<S>) -> Self {
        Self { id_to_pool: RwLock::new(HashMap::new()), counter: AtomicU32::new(0), storage }
    }

    fn id_to_pool_index(&self, x: &tracing::Id) -> u32 {
        let id_to_pool_r = self.id_to_pool.read().unwrap();
        match id_to_pool_r.get(x) {
            Some(x) => *x,
            None => {
                error!(
                    "CRITICAL \nno known map of {x:?} to pool id, this shouldn't happen. Will \
                     become child of root instead. "
                );
                0
            }
        }
    }
}

impl<S: Subscriber, S2: Storage + 'static> Layer<S> for TreeLayer<S2> {
    fn on_new_span(
        &self, attrs: &tracing::span::Attributes<'_>, id: &tracing::span::Id,
        ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        let parent: u32;
        if let Some(x) = attrs.parent() {
            parent = self.id_to_pool_index(x);
        } else if attrs.is_root() {
            parent = 0;
        } else if attrs.is_contextual() {
            parent = match ctx.current_span().id() {
                Some(x) => self.id_to_pool_index(x),
                None => 0,
            };
        } else {
            unreachable!()
        }
        let mut visitor = EventVisitor::new();
        attrs.values().record(&mut visitor);
        let pool_id: u32 = self.counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1; // the atomic returns the previous value, so add one here too
        self.id_to_pool.write().unwrap().insert(id.clone(), pool_id);
        let sent_attrs = visitor.attrs.into_iter().map(|x| (x.0.to_string(), x.1)).collect();
        self.storage.new_span(parent, sent_attrs, attrs.metadata());
    }
    fn on_event(&self, event: &tracing::Event<'_>, ctx: tracing_subscriber::layer::Context<'_, S>) {
        let parent: u32;
        if let Some(x) = event.parent() {
            parent = self.id_to_pool_index(x);
        } else if event.is_root() {
            parent = 0;
        } else if event.is_contextual() {
            parent = match ctx.current_span().id() {
                Some(x) => self.id_to_pool_index(x),
                None => 0,
            };
        } else {
            unreachable!()
        }

        let mut visitor = EventVisitor::new();
        event.record(&mut visitor);
        self.counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.storage.new_event(
            parent,
            visitor.attrs.into_iter().map(|x| (x.0.to_string(), x.1)).collect(),
            event.metadata(),
        );
    }
    fn on_close(&self, id: tracing::span::Id, _ctx: tracing_subscriber::layer::Context<'_, S>) {
        self.id_to_pool.write().unwrap().remove(&id);
    }
}
/// A value which can be saved into an entrace file.
///
/// The canonical field order is:
/// `String`, `Bytes`, `Bool`, `Float`, `U64`, `I64`, `U128`, `I128`
#[derive(serde::Deserialize, serde::Serialize, Clone, Debug)]
pub enum EnValue {
    String(String),
    Bytes(Vec<u8>),
    Bool(bool),
    Float(f64),
    U64(u64),
    I64(i64),
    U128(u128),
    I128(i128),
}
impl EnValue {
    pub fn as_ref(&'_ self) -> EnValueRef<'_> {
        match self {
            EnValue::String(q) => EnValueRef::String(q.as_str()),
            EnValue::Bool(q) => EnValueRef::Bool(*q),
            EnValue::Bytes(q) => EnValueRef::Bytes(q.as_ref()),
            EnValue::Float(q) => EnValueRef::Float(*q),
            EnValue::U64(q) => EnValueRef::U64(*q),
            EnValue::I64(q) => EnValueRef::I64(*q),
            EnValue::U128(q) => EnValueRef::U128(*q),
            EnValue::I128(q) => EnValueRef::I128(*q),
        }
    }
}

/// Container for borrowed versions of [EnValue]'s data, where it makes sense.
///
/// The canonical field order is:
/// `String`, `Bytes`, `Bool`, `Float`, `U64`, `I64`, `U128`, `I128`
#[derive(serde::Deserialize, serde::Serialize, Clone, Debug)]
pub enum EnValueRef<'a> {
    String(&'a str),
    Bytes(&'a [u8]),
    Bool(bool),
    Float(f64),
    U64(u64),
    I64(i64),
    U128(u128),
    I128(i128),
}
impl<'a> EnValueRef<'a> {
    pub fn to_owned(&self) -> EnValue {
        match self {
            EnValueRef::String(q) => EnValue::String(q.to_string()),
            EnValueRef::Bool(q) => EnValue::Bool(*q),
            EnValueRef::Bytes(items) => EnValue::Bytes(items.to_vec()),
            EnValueRef::Float(q) => EnValue::Float(*q),
            EnValueRef::U64(q) => EnValue::U64(*q),
            EnValueRef::I64(q) => EnValue::I64(*q),
            EnValueRef::U128(q) => EnValue::U128(*q),
            EnValueRef::I128(q) => EnValue::I128(*q),
        }
    }
    pub fn into_owned(self) -> EnValue {
        match self {
            EnValueRef::String(q) => EnValue::String(q.to_string()),
            EnValueRef::Bool(q) => EnValue::Bool(q),
            EnValueRef::Bytes(items) => EnValue::Bytes(items.to_vec()),
            EnValueRef::Float(q) => EnValue::Float(q),
            EnValueRef::U64(q) => EnValue::U64(q),
            EnValueRef::I64(q) => EnValue::I64(q),
            EnValueRef::U128(q) => EnValue::U128(q),
            EnValueRef::I128(q) => EnValue::I128(q),
        }
    }
}

impl std::fmt::Display for EnValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EnValue::String(q) => q.fmt(f),
            EnValue::Bool(q) => q.fmt(f),
            EnValue::Bytes(q) => write!(f, "{q:?}"),
            EnValue::Float(q) => q.fmt(f),
            EnValue::U64(q) => q.fmt(f),
            EnValue::I64(q) => q.fmt(f),
            EnValue::U128(q) => q.fmt(f),
            EnValue::I128(q) => q.fmt(f),
        }
    }
}

impl<'a> std::fmt::Display for EnValueRef<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EnValueRef::String(q) => q.fmt(f),
            EnValueRef::Bool(q) => q.fmt(f),
            EnValueRef::Bytes(q) => write!(f, "{q:?}"),
            EnValueRef::Float(q) => q.fmt(f),
            EnValueRef::U64(q) => q.fmt(f),
            EnValueRef::I64(q) => q.fmt(f),
            EnValueRef::U128(q) => q.fmt(f),
            EnValueRef::I128(q) => q.fmt(f),
        }
    }
}

struct EventVisitor {
    pub attrs: Vec<(&'static str, EnValue)>,
}
impl tracing::field::Visit for EventVisitor {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        self.new_attr(field, EnValue::String(format!("{value:?}")))
    }
    fn record_bool(&mut self, field: &tracing::field::Field, value: bool) {
        self.new_attr(field, EnValue::Bool(value))
    }
    fn record_bytes(&mut self, field: &tracing::field::Field, value: &[u8]) {
        self.new_attr(field, EnValue::Bytes(value.into()))
    }
    fn record_error(
        &mut self, field: &tracing::field::Field, value: &(dyn std::error::Error + 'static),
    ) {
        self.new_attr(field, EnValue::String(format!("{value:?}")))
    }
    fn record_f64(&mut self, field: &tracing::field::Field, value: f64) {
        self.new_attr(field, EnValue::Float(value))
    }
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        self.new_attr(field, EnValue::String(value.into()))
    }
    fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
        self.new_attr(field, EnValue::U64(value))
    }
    fn record_u128(&mut self, field: &tracing::field::Field, value: u128) {
        self.new_attr(field, EnValue::U128(value))
    }
    fn record_i64(&mut self, field: &tracing::field::Field, value: i64) {
        self.new_attr(field, EnValue::I64(value))
    }
    fn record_i128(&mut self, field: &tracing::field::Field, value: i128) {
        self.new_attr(field, EnValue::I128(value))
    }
}
impl EventVisitor {
    pub fn new() -> Self {
        Self { attrs: vec![] }
    }
    pub fn new_attr(&mut self, field: &tracing::field::Field, value: EnValue) {
        self.attrs.push((field.name(), value));
    }
}

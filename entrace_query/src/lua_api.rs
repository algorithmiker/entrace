use std::{
    cell::RefCell,
    cmp::Ordering,
    collections::HashMap,
    ops::RangeInclusive,
    rc::Rc,
    sync::{Arc, RwLock, RwLockReadGuard},
};

use anyhow::bail;
use entrace_core::{EnValue, EnValueRef, LevelContainer, LogProviderError, MetadataRefContainer};
use memchr::memmem::Finder;
use mlua::{ExternalError, IntoLua, Lua, Table, Value};

use crate::{
    QueryError, TraceProvider,
    lua_value::{LuaValueRef, LuaValueRefRef},
};
fn level_to_u8(level: &entrace_core::LevelContainer) -> u8 {
    match level {
        LevelContainer::Trace => 1,
        LevelContainer::Debug => 2,
        LevelContainer::Info => 3,
        LevelContainer::Warn => 4,
        LevelContainer::Error => 5,
    }
}
fn make_oob_error(index: u32, len: usize) -> mlua::Error {
    let actual = len as u32;
    let e = QueryError::OutOfBounds { index, actual };
    mlua::Error::ExternalError(Arc::new(e))
}

fn to_lua_error(x: LogProviderError) -> mlua::Error {
    mlua::Error::ExternalError(Arc::new(x))
}

pub fn en_span_range(range: &RangeInclusive<u32>) -> mlua::Result<(u32, u32)> {
    Ok((*range.start(), *range.end()))
}
pub fn en_log(x: mlua::Value) -> mlua::Result<()> {
    println!("Lua log: {x:?}");
    Ok(())
}

pub fn en_children(
    trace_provider: Arc<RwLock<TraceProvider>>,
) -> impl Fn(&Lua, u32) -> mlua::Result<Vec<u32>> {
    move |_lua: &Lua, id: u32| {
        let tcc = trace_provider.read().unwrap();
        let c = tcc.children(id).map_err(to_lua_error)?;
        Ok(c.to_vec())
    }
}
pub fn en_child_cnt(
    trace_provider: Arc<RwLock<TraceProvider>>,
) -> impl Fn(&Lua, u32) -> mlua::Result<usize> {
    move |_lua: &Lua, id: u32| {
        let tcc = trace_provider.read().unwrap();
        let c = tcc.children(id).map_err(to_lua_error)?;
        Ok(c.len())
    }
}

pub fn en_span_cnt(
    trace_provider: Arc<RwLock<TraceProvider>>,
) -> impl Fn(&Lua, ()) -> mlua::Result<usize> {
    move |_lua: &Lua, _: ()| Ok(trace_provider.read().unwrap().len())
}

pub fn en_metadata(
    trace_provider: Arc<RwLock<TraceProvider>>,
) -> impl Fn(&Lua, u32) -> mlua::Result<Table> {
    move |lua: &Lua, id: u32| {
        let tcc = trace_provider.read().unwrap();
        let c = tcc.meta(id).map_err(to_lua_error)?;
        let MetadataRefContainer { name, level, file, line, target, module_path } = c;

        let table = lua.create_table()?;
        table.set("name", name)?;
        table.set("level", level_to_u8(&level))?;
        table.set("file", file)?;
        table.set("line", line)?;
        table.set("target", target)?;
        table.set("module_path", module_path)?;
        Ok(table)
    }
}

pub fn en_metadata_name(
    trace_provider: Arc<RwLock<TraceProvider>>,
) -> impl Fn(&Lua, u32) -> mlua::Result<String> {
    move |_lua: &Lua, id: u32| {
        let tcc = trace_provider.read().unwrap();
        let c = tcc.meta(id).map_err(to_lua_error)?;
        let MetadataRefContainer { name, .. } = c;
        Ok(name.to_string())
    }
}

pub fn en_metadata_level(
    trace_provider: Arc<RwLock<TraceProvider>>,
) -> impl Fn(&Lua, u32) -> mlua::Result<u8> {
    move |_lua: &Lua, id: u32| {
        let tcc = trace_provider.read().unwrap();
        let c = tcc.meta(id).map_err(to_lua_error)?;
        let MetadataRefContainer { level, .. } = c;
        Ok(level_to_u8(&level))
    }
}

pub fn en_metadata_file(
    trace_provider: Arc<RwLock<TraceProvider>>,
) -> impl Fn(&Lua, u32) -> mlua::Result<Option<Value>> {
    move |lua: &Lua, id: u32| {
        let tcc = trace_provider.read().unwrap();
        let c = tcc.meta(id).map_err(to_lua_error)?;
        let MetadataRefContainer { file, .. } = c;
        let filef = file.map(|x| x.into_lua(lua).unwrap());
        Ok(filef)
    }
}
pub fn en_metadata_line(
    trace_provider: Arc<RwLock<TraceProvider>>,
) -> impl Fn(&Lua, u32) -> mlua::Result<Option<Value>> {
    move |lua: &Lua, id: u32| {
        let tcc = trace_provider.read().unwrap();
        let c = tcc.meta(id).map_err(to_lua_error)?;
        let MetadataRefContainer { line, .. } = c;
        Ok(line.map(|x| x.into_lua(lua).unwrap()))
    }
}

pub fn en_metadata_target(
    trace_provider: Arc<RwLock<TraceProvider>>,
) -> impl Fn(&Lua, u32) -> mlua::Result<String> {
    move |_lua: &Lua, id: u32| {
        let tcc = trace_provider.read().unwrap();
        let c = tcc.meta(id).map_err(to_lua_error)?;
        let MetadataRefContainer { target, .. } = c;
        Ok(target.to_string())
    }
}

pub fn en_metadata_module_path(
    trace_provider: Arc<RwLock<TraceProvider>>,
) -> impl Fn(&Lua, u32) -> mlua::Result<Option<mlua::Value>> {
    move |lua: &Lua, id: u32| {
        let tcc = trace_provider.read().unwrap();
        let c = tcc.meta(id).map_err(to_lua_error)?;
        let MetadataRefContainer { module_path, .. } = c;
        Ok(module_path.map(|x| x.into_lua(lua).unwrap()))
    }
}

pub fn en_attrs(
    trace_provider: Arc<RwLock<TraceProvider>>,
) -> impl Fn(&Lua, u32) -> mlua::Result<Table> {
    move |lua: &Lua, id: u32| {
        let tcc = trace_provider.read().unwrap();
        let c = tcc.attrs(id).map_err(to_lua_error)?;
        let table = lua.create_table_with_capacity(0, c.len())?;
        for (key, value) in c {
            table.set(key, LuaValueRef(value))?;
        }
        Ok(table)
    }
}

pub fn en_attr_names(
    trace_provider: Arc<RwLock<TraceProvider>>,
) -> impl Fn(&Lua, u32) -> mlua::Result<Vec<Value>> {
    move |lua: &Lua, id: u32| {
        let tcc = trace_provider.read().unwrap();
        let c = tcc.attrs(id).map_err(to_lua_error)?;
        let mut names = Vec::with_capacity(c.len());
        for (key, _) in c {
            names.push(key.into_lua(lua)?);
        }
        Ok(names)
    }
}

pub fn en_attr_values(
    trace_provider: Arc<RwLock<TraceProvider>>,
) -> impl Fn(&Lua, u32) -> mlua::Result<Vec<Value>> {
    move |lua: &Lua, id: u32| {
        let tcc = trace_provider.read().unwrap();
        let c = tcc.attrs(id).map_err(to_lua_error)?;
        let mut values = Vec::with_capacity(c.len());
        for (_, value) in c {
            values.push(LuaValueRef(value).into_lua(lua)?);
        }
        Ok(values)
    }
}

pub fn en_attr_by_idx(
    trace_provider: Arc<RwLock<TraceProvider>>,
) -> impl Fn(&Lua, (u32, usize)) -> mlua::Result<(mlua::String, mlua::Value)> {
    move |lua: &Lua, (id, idx): (u32, usize)| {
        let tcc = trace_provider.read().unwrap();
        let c = tcc.attrs(id).map_err(to_lua_error)?;
        let (k, v) = c.get(idx).ok_or_else(|| make_oob_error(idx as u32, c.len()))?;
        Ok((lua.create_string(k)?, LuaValueRefRef(v).into_lua(lua)?))
    }
}

pub fn en_attr_by_name(
    trace_provider: Arc<RwLock<TraceProvider>>,
) -> impl Fn(&Lua, (u32, String)) -> mlua::Result<mlua::Value> {
    move |lua: &Lua, (id, key): (u32, String)| {
        let tcc = trace_provider.read().unwrap();
        let attrs = tcc.attrs(id).map_err(to_lua_error)?;
        let attr = attrs.iter().find(|(k, _)| *k == key);
        if let Some(attr) = attr { LuaValueRefRef(&attr.1).into_lua(lua) } else { Ok(mlua::Nil) }
    }
}

pub fn en_attr_name(
    trace_provider: Arc<RwLock<TraceProvider>>,
) -> impl Fn(&Lua, (u32, usize)) -> mlua::Result<mlua::String> {
    move |lua: &Lua, (id, idx): (u32, usize)| {
        let tcc = trace_provider.read().unwrap();
        let c = tcc.attrs(id).map_err(to_lua_error)?;
        let (k, _) = c.get(idx).ok_or_else(|| make_oob_error(idx as u32, c.len()))?;
        lua.create_string(k)
    }
}

pub fn en_attr_value(
    trace_provider: Arc<RwLock<TraceProvider>>,
) -> impl Fn(&Lua, (u32, usize)) -> mlua::Result<mlua::Value> {
    move |lua: &Lua, (id, idx): (u32, usize)| {
        let tcc = trace_provider.read().unwrap();
        let c = tcc.attrs(id).map_err(to_lua_error)?;
        let (_, v) = c.get(idx).ok_or_else(|| make_oob_error(idx as u32, c.len()))?;
        LuaValueRefRef(v).into_lua(lua)
    }
}

pub fn en_as_string(
    trace_provider: Arc<RwLock<TraceProvider>>,
) -> impl Fn(&Lua, u32) -> mlua::Result<mlua::String> {
    move |lua: &Lua, id: u32| {
        let tcc = trace_provider.read().unwrap();
        let attrs = tcc.attrs(id).map_err(to_lua_error)?;
        let meta = tcc.meta(id).map_err(to_lua_error)?;
        let children = tcc.children(id).map_err(to_lua_error)?;

        #[derive(Debug)]
        #[allow(dead_code)]
        struct Entry<'a> {
            meta: &'a MetadataRefContainer<'a>,
            attrs: &'a Vec<(&'a str, EnValueRef<'a>)>,
            children: &'a [u32],
        }
        let entry = Entry { meta: &meta, attrs: &attrs, children };
        let s = format!("{entry:?}");
        lua.create_string(s)
    }
}

pub fn en_contains_anywhere(
    trace_provider: Arc<RwLock<TraceProvider>>, finder_cache: Rc<RefCell<HashMap<String, Finder>>>,
) -> impl Fn(&Lua, (u32, String)) -> mlua::Result<bool> {
    move |_lua: &Lua, (id, needle): (u32, String)| {
        let tcc = trace_provider.read().unwrap();
        let attrs = tcc.attrs(id).map_err(to_lua_error)?;
        let meta = tcc.meta(id).map_err(to_lua_error)?;
        let children = tcc.children(id).map_err(to_lua_error)?;
        let mut finder_w = finder_cache.borrow_mut();
        let finder = if let Some(q) = finder_w.get(&needle) {
            q
        } else {
            finder_w.insert(needle.clone(), memchr::memmem::Finder::new(&needle).into_owned());
            finder_w.get(&needle).unwrap()
        };
        #[derive(Debug)]
        #[allow(dead_code)]
        struct Entry<'a> {
            meta: &'a MetadataRefContainer<'a>,
            attrs: &'a Vec<(&'a str, EnValueRef<'a>)>,
            children: &'a [u32],
        }
        let entry = Entry { meta: &meta, attrs: &attrs, children };
        let s = format!("{entry:?}");
        let contains = finder.find(s.as_bytes());
        Ok(contains.is_some())
    }
}

fn meta_matches(
    meta: &MetadataRefContainer, target: &str, comparator: Ordering, value: &EnValue,
) -> anyhow::Result<bool> {
    fn string_eq(a: &str, value: &EnValue, comparator: std::cmp::Ordering) -> bool {
        match value {
            EnValue::String(b) => a.cmp(b) == comparator,
            _ => false,
        }
    }
    fn opt_string_eq(a: Option<&str>, value: &EnValue, comparator: Ordering) -> bool {
        let Some(a) = a else { return false };
        match value {
            EnValue::String(b) => a.cmp(b) == comparator,
            _ => false,
        }
    }
    match target {
        "name" => Ok(string_eq(meta.name, value, comparator)),
        "target" => Ok(string_eq(meta.target, value, comparator)),
        "level" => {
            let asu8 = match value {
                EnValue::U64(x) => *x as u8,
                EnValue::I64(x) => *x as u8,
                _ => return Ok(false),
            };
            Ok((meta.level as u8).cmp(&asu8) == comparator)
        }
        "module_path" => Ok(opt_string_eq(meta.module_path, value, comparator)),
        "file" => Ok(opt_string_eq(meta.file, value, comparator)),
        "line" => {
            let converted = match value {
                EnValue::Float(a) => *a as u32,
                EnValue::U64(a) => *a as u32,
                EnValue::I64(a) => *a as u32,
                _ => return Ok(false),
            };
            let Some(line) = meta.line else { return Ok(false) };
            Ok(line.cmp(&converted) == comparator)
        }
        x => bail!("Bad meta field {x}"),
    }
}

fn values_match(comparator: std::cmp::Ordering, span_value: &EnValueRef, value: &EnValue) -> bool {
    match value {
        EnValue::String(a) => match span_value {
            EnValueRef::String(b) => b.cmp(&a.as_str()) == comparator,
            _ => false,
        },
        EnValue::Bool(a) => match span_value {
            EnValueRef::Bool(b) => b.cmp(a) == comparator,
            _ => false,
        },
        EnValue::Float(a) => match span_value {
            EnValueRef::Float(b) => b.total_cmp(a) == comparator,
            _ => false,
        },
        EnValue::U64(a) => {
            let span_value_converted = match span_value {
                EnValueRef::U64(x) => *x,
                EnValueRef::I64(x) => *x as u64,
                EnValueRef::U128(x) => *x as u64,
                EnValueRef::I128(x) => *x as u64,
                _ => return false,
            };
            span_value_converted.cmp(a) == comparator
        }
        EnValue::I64(a) => {
            let span_value_converted = match span_value {
                EnValueRef::U64(x) => *x as i64,
                EnValueRef::I64(x) => *x,
                EnValueRef::U128(x) => *x as i64,
                EnValueRef::I128(x) => *x as i64,
                _ => return false,
            };
            span_value_converted.cmp(a) == comparator
        }
        // we explicitly don't construct these
        EnValue::U128(_) => false,
        EnValue::I128(_) => false,
        // table->bytes is not handled for now
        EnValue::Bytes(_) => false,
    }
}

pub fn filter_inner(
    tcc: RwLockReadGuard<TraceProvider>, ids: impl Iterator<Item = u32>, target: &str,
    target_is_meta: bool, relation: Ordering, en_value: EnValue,
) -> mlua::Result<Vec<u32>> {
    let mut returned = Vec::with_capacity(128);
    for id in ids {
        if target_is_meta {
            let meta = tcc.meta(id).unwrap();
            let matches_here =
                meta_matches(&meta, target, relation, &en_value).map_err(|x| x.into_lua_err())?;
            if matches_here {
                returned.push(id);
            }
        } else {
            let attrs = tcc.attrs(id).unwrap();
            let Some((_name, target_here)) = attrs.iter().find(|(name, _)| *name == target) else {
                continue;
            };
            let matches_here = values_match(relation, target_here, &en_value);
            if matches_here {
                returned.push(id);
            }
        }
    }
    Ok(returned)
}

fn parse_filter_desc(desc: mlua::Table) -> mlua::Result<(String, bool, Ordering, EnValue)> {
    use anyhow::anyhow;
    let Ok(raw_target): mlua::Result<String> = desc.get("target") else {
        return Err(anyhow!("Filter target must be a string").into_lua_err());
    };
    let mut target: &str = raw_target.as_str();
    let mut target_is_meta = false;
    if let Some(stripped) = raw_target.strip_prefix("meta.") {
        target = stripped;
        target_is_meta = true;
    }
    let Ok(rel): mlua::Result<String> = desc.get("relation") else {
        return Err(anyhow!("Filter relation must be a string").into_lua_err());
    };
    let relation = match rel.as_str() {
        "GT" => Ordering::Greater,
        "LT" => Ordering::Less,
        "EQ" => Ordering::Equal,
        x => return Err(anyhow::anyhow!("Bad filter relation {x}").into_lua_err()),
    };

    let value: Value = desc.get("value").unwrap();
    let en_value = match value {
        Value::Boolean(f) => EnValue::Bool(f),
        Value::Integer(k) => EnValue::I64(k),
        Value::Number(z) => EnValue::Float(z),
        Value::String(ref q) => EnValue::String(q.to_string_lossy()),
        x => {
            return Err(anyhow!("Cannot convert value {x:?} to EnValue").into_lua_err());
        }
    };
    Ok((target.into(), target_is_meta, relation, en_value))
}

/// en_filter(list<id>, compare_desc)
/// where compare_desc is a table:
///   target: name of variable eg. "message" or "meta.filename"
///   relation: a relation, one of "EQ", "LT", "GT"
///   value: a constant to compare with
/// Returns the ids of the spans matching the compare_desc.
pub fn en_filter(
    trace_provider: Arc<RwLock<TraceProvider>>,
) -> impl Fn(&Lua, (Vec<u32>, Table)) -> mlua::Result<Vec<u32>> {
    move |_lua: &Lua, (ids, desc): (Vec<u32>, Table)| {
        let tcc = trace_provider.read().unwrap();
        let (target, target_is_meta, relation, en_value) = parse_filter_desc(desc)?;
        filter_inner(tcc, ids.iter().copied(), &target, target_is_meta, relation, en_value)
    }
}
/// Same as en_filter, but accets a range start and range end instead
pub fn en_filter_range(
    trace_provider: Arc<RwLock<TraceProvider>>,
) -> impl Fn(&Lua, (u32, u32, Table)) -> mlua::Result<Vec<u32>> {
    move |_lua: &Lua, (start, end, desc): (u32, u32, Table)| {
        let tcc = trace_provider.read().unwrap();
        let (target, target_is_meta, relation, en_value) = parse_filter_desc(desc)?;
        filter_inner(tcc, start..=end, &target, target_is_meta, relation, en_value)
    }
}

pub fn setup_lua(
    lua: &mut Lua, range: RangeInclusive<u32>, trace: Arc<RwLock<TraceProvider>>,
    finder_cache: Rc<RefCell<HashMap<String, Finder<'static>>>>,
) -> Result<(), mlua::Error> {
    let globals = lua.globals();
    let en_range = lua.create_function(move |_state, _: ()| en_span_range(&range));
    globals.set("en_span_range", en_range?)?;
    globals.set("en_log", lua.create_function(move |_, x| en_log(x))?)?;
    globals.set("en_children", lua.create_function(en_children(trace.clone()))?)?;
    globals.set("en_child_cnt", lua.create_function(en_child_cnt(trace.clone()))?)?;
    globals.set("en_span_cnt", lua.create_function(en_span_cnt(trace.clone()))?)?;
    globals.set("en_metadata_table", lua.create_function(en_metadata(trace.clone()))?)?;
    globals.set("en_metadata_name", lua.create_function(en_metadata_name(trace.clone()))?)?;
    globals.set("en_metadata_level", lua.create_function(en_metadata_level(trace.clone()))?)?;
    globals.set("en_metadata_file", lua.create_function(en_metadata_file(trace.clone()))?)?;
    globals.set("en_metadata_line", lua.create_function(en_metadata_line(trace.clone()))?)?;
    globals.set("en_metadata_target", lua.create_function(en_metadata_target(trace.clone()))?)?;
    globals.set(
        "en_metadata_module_path",
        lua.create_function(en_metadata_module_path(trace.clone()))?,
    )?;
    globals.set("en_attrs", lua.create_function(en_attrs(trace.clone()))?)?;
    globals.set("en_attr_names", lua.create_function(en_attr_names(trace.clone()))?)?;
    globals.set("en_attr_values", lua.create_function(en_attr_values(trace.clone()))?)?;
    globals.set("en_attr_by_idx", lua.create_function(en_attr_by_idx(trace.clone()))?)?;
    globals.set("en_attr_by_name", lua.create_function(en_attr_by_name(trace.clone()))?)?;
    globals.set("en_attr_name", lua.create_function(en_attr_name(trace.clone()))?)?;
    globals.set("en_attr_value", lua.create_function(en_attr_value(trace.clone()))?)?;
    globals.set("en_as_string", lua.create_function(en_as_string(trace.clone()))?)?;
    let fc = finder_cache.clone();
    globals.set(
        "en_contains_anywhere",
        lua.create_function(en_contains_anywhere(trace.clone(), fc))?,
    )?;
    globals.set("en_filter", lua.create_function(en_filter(trace.clone()))?)?;
    globals.set("en_filter_range", lua.create_function(en_filter_range(trace.clone()))?)?;

    Ok(())
}

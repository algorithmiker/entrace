use std::{
    cell::RefCell,
    cmp::Ordering,
    collections::HashMap,
    error::Error,
    ops::RangeInclusive,
    rc::Rc,
    sync::{Arc, RwLock},
};

use anyhow::bail;
use entrace_core::{
    EnValue, EnValueRef, LevelContainer, LogProvider, LogProviderError, LogProviderResult,
    MetadataRefContainer,
};
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

fn to_lua_err(x: impl Error + Send + Sync + 'static) -> mlua::Error {
    x.into_lua_err()
}

pub fn en_span_range(range: &RangeInclusive<u32>) -> mlua::Result<(u32, u32)> {
    Ok((*range.start(), *range.end()))
}
pub fn en_log(x: mlua::Value) -> mlua::Result<()> {
    println!("Lua log: {x:?}");
    Ok(())
}

pub fn en_children(tcc: &impl LogProvider) -> impl Fn(u32) -> Result<Vec<u32>, LogProviderError> {
    move |id: u32| tcc.children(id).map(|x| x.into())
}

pub fn en_child_cnt(tcc: &impl LogProvider) -> impl Fn(u32) -> Result<usize, LogProviderError> {
    move |id: u32| Ok(tcc.children(id)?.len())
}

pub fn en_span_cnt(tcc: &impl LogProvider) -> impl Fn(()) -> mlua::Result<usize> {
    move |_: ()| Ok(tcc.len())
}

pub fn en_metadata_table(tcc: &impl LogProvider, lua: &Lua) -> impl Fn(u32) -> mlua::Result<Table> {
    move |id: u32| {
        let c = tcc.meta(id).map_err(to_lua_err)?;
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
    tcc: &impl LogProvider,
) -> impl Fn(u32) -> Result<String, LogProviderError> {
    move |id: u32| Ok(tcc.meta(id)?.name.to_string())
}

pub fn en_metadata_level(tcc: &impl LogProvider) -> impl Fn(u32) -> LogProviderResult<u8> {
    move |id: u32| {
        let c = tcc.meta(id)?;
        Ok(level_to_u8(&c.level))
    }
}

pub fn en_metadata_file(
    tcc: &impl LogProvider,
) -> impl Fn(u32) -> LogProviderResult<Option<String>> {
    move |id: u32| {
        let c = tcc.meta(id)?;
        Ok(c.file.map(|x| x.to_owned()))
    }
}
pub fn en_metadata_line(tcc: &impl LogProvider) -> impl Fn(u32) -> mlua::Result<Option<u32>> {
    move |id: u32| {
        let c = tcc.meta(id).map_err(to_lua_err)?;
        Ok(c.line)
    }
}

pub fn en_metadata_target(tcc: &impl LogProvider) -> impl Fn(u32) -> LogProviderResult<String> {
    move |id: u32| {
        let c = tcc.meta(id)?;
        Ok(c.target.to_string())
    }
}

pub fn en_metadata_module_path(
    tcc: &impl LogProvider,
) -> impl Fn(u32) -> LogProviderResult<Option<String>> {
    move |id: u32| {
        let c = tcc.meta(id)?;
        Ok(c.module_path.map(|x| x.to_string()))
    }
}

pub fn en_attrs(tcc: &impl LogProvider, lua: &Lua) -> impl Fn(u32) -> mlua::Result<Table> {
    move |id: u32| {
        let c = tcc.attrs(id).map_err(to_lua_err)?;
        let table = lua.create_table_with_capacity(0, c.len())?;
        for (key, value) in c {
            table.set(key, LuaValueRef(value))?;
        }
        Ok(table)
    }
}

pub fn en_attr_names(
    tcc: &impl LogProvider, lua: &Lua,
) -> impl Fn(u32) -> mlua::Result<Vec<Value>> {
    move |id: u32| {
        let c = tcc.attrs(id).map_err(to_lua_err)?;
        let mut names = Vec::with_capacity(c.len());
        for (key, _) in c {
            names.push(key.into_lua(lua)?);
        }
        Ok(names)
    }
}

pub fn en_attr_values(
    tcc: &impl LogProvider, lua: &Lua,
) -> impl Fn(u32) -> mlua::Result<Vec<Value>> {
    move |id: u32| {
        let c = tcc.attrs(id).map_err(to_lua_err)?;
        let mut values = Vec::with_capacity(c.len());
        for (_, value) in c {
            values.push(LuaValueRef(value).into_lua(lua)?);
        }
        Ok(values)
    }
}

pub fn en_attr_by_idx(
    tcc: &impl LogProvider, lua: &Lua,
) -> impl Fn((u32, usize)) -> mlua::Result<(mlua::String, mlua::Value)> {
    move |(id, idx): (u32, usize)| {
        let c = tcc.attrs(id).map_err(to_lua_err)?;
        let (k, v) = c.get(idx).ok_or_else(|| make_oob_error(idx as u32, c.len()))?;
        Ok((lua.create_string(k)?, LuaValueRefRef(v).into_lua(lua)?))
    }
}

pub fn en_attr_by_name(
    tcc: &impl LogProvider, lua: &Lua,
) -> impl Fn((u32, String)) -> mlua::Result<mlua::Value> {
    move |(id, key): (u32, String)| {
        let attrs = tcc.attrs(id).map_err(to_lua_err)?;
        let attr = attrs.iter().find(|(k, _)| *k == key);
        if let Some(attr) = attr { LuaValueRefRef(&attr.1).into_lua(lua) } else { Ok(mlua::Nil) }
    }
}

pub fn en_attr_name(
    tcc: &impl LogProvider, lua: &Lua,
) -> impl Fn((u32, usize)) -> mlua::Result<mlua::String> {
    move |(id, idx): (u32, usize)| {
        let c = tcc.attrs(id).map_err(to_lua_err)?;
        let (k, _) = c.get(idx).ok_or_else(|| make_oob_error(idx as u32, c.len()))?;
        lua.create_string(k)
    }
}

pub fn en_attr_value(
    tcc: &impl LogProvider, lua: &Lua,
) -> impl Fn((u32, usize)) -> mlua::Result<mlua::Value> {
    move |(id, idx): (u32, usize)| {
        let c = tcc.attrs(id).map_err(to_lua_err)?;
        let (_, v) = c.get(idx).ok_or_else(|| make_oob_error(idx as u32, c.len()))?;
        LuaValueRefRef(v).into_lua(lua)
    }
}

pub fn en_as_string(tcc: &impl LogProvider) -> impl Fn(u32) -> LogProviderResult<String> {
    move |id: u32| {
        let attrs = tcc.attrs(id)?;
        let meta = tcc.meta(id)?;
        let children = tcc.children(id)?;

        #[derive(Debug)]
        #[allow(dead_code)]
        struct Entry<'a> {
            meta: &'a MetadataRefContainer<'a>,
            attrs: &'a Vec<(&'a str, EnValueRef<'a>)>,
            children: &'a [u32],
        }
        let entry = Entry { meta: &meta, attrs: &attrs, children };
        Ok(format!("{entry:?}"))
    }
}

pub fn en_contains_anywhere(
    tcc: &impl LogProvider, finder_cache: Rc<RefCell<HashMap<String, Finder>>>,
) -> impl Fn((u32, String)) -> LogProviderResult<bool> {
    move |(id, needle): (u32, String)| {
        let entry = en_as_string(tcc)(id)?;

        let mut finder_w = finder_cache.borrow_mut();
        let finder = if let Some(q) = finder_w.get(&needle) {
            q
        } else {
            finder_w.insert(needle.clone(), memchr::memmem::Finder::new(&needle).into_owned());
            finder_w.get(&needle).unwrap()
        };
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
    tcc: &impl LogProvider, ids: impl Iterator<Item = u32>, target: &str, target_is_meta: bool,
    relation: Ordering, en_value: EnValue,
) -> anyhow::Result<Vec<u32>> {
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
pub fn en_filter(tcc: &impl LogProvider) -> impl Fn((Vec<u32>, Table)) -> anyhow::Result<Vec<u32>> {
    move |(ids, desc): (Vec<u32>, Table)| {
        let (target, target_is_meta, relation, en_value) = parse_filter_desc(desc)?;
        filter_inner(tcc, ids.iter().copied(), &target, target_is_meta, relation, en_value)
    }
}
/// Same as en_filter, but accets a range start and range end instead
pub fn en_filter_range(
    tcc: &impl LogProvider,
) -> impl Fn((u32, u32, Table)) -> anyhow::Result<Vec<u32>> {
    move |(start, end, desc): (u32, u32, Table)| {
        let (target, target_is_meta, relation, en_value) = parse_filter_desc(desc)?;
        filter_inner(tcc, start..=end, &target, target_is_meta, relation, en_value)
    }
}

struct DynAdapter<'a>(&'a dyn LogProvider);
impl<'a> LogProvider for DynAdapter<'a> {
    fn children(&self, x: u32) -> Result<&[u32], LogProviderError> {
        self.0.children(x)
    }

    fn parent(&self, x: u32) -> Result<u32, LogProviderError> {
        self.0.parent(x)
    }

    fn attrs(&'_ self, x: u32) -> Result<Vec<(&'_ str, EnValueRef<'_>)>, LogProviderError> {
        self.0.attrs(x)
    }

    fn header(&'_ self, x: u32) -> Result<entrace_core::Header<'_>, LogProviderError> {
        self.0.header(x)
    }

    fn meta(&'_ self, x: u32) -> Result<MetadataRefContainer<'_>, LogProviderError> {
        self.0.meta(x)
    }

    fn len(&self) -> usize {
        self.0.len()
    }
}

macro_rules! lua_setup_with_wrappers {
    ($lua: expr, $trace: expr, $finder_cache: expr, $range: expr, $lua_wrap: ident, $lua_wrap2: ident) => {
        let globals = $lua.globals();
        let en_range = $lua.create_function(move |_state, _: ()| en_span_range(&$range));
        globals.set("en_span_range", en_range?)?;
        globals.set("en_log", $lua.create_function(move |_, x| en_log(x))?)?;
        let t = $trace.clone();
        globals.set("en_children", $lua.create_function($lua_wrap!(t, u32, en_children))?)?;
        globals.set("en_child_cnt", $lua.create_function($lua_wrap!(t, u32, en_child_cnt))?)?;
        globals.set("en_span_cnt", $lua.create_function($lua_wrap!(t, (), en_span_cnt))?)?;
        globals.set(
            "en_metadata_table",
            $lua.create_function($lua_wrap2!(t, u32, en_metadata_table))?,
        )?;
        globals
            .set("en_metadata_name", $lua.create_function($lua_wrap!(t, u32, en_metadata_name))?)?;
        globals.set(
            "en_metadata_level",
            $lua.create_function($lua_wrap!(t, u32, en_metadata_level))?,
        )?;
        globals
            .set("en_metadata_file", $lua.create_function($lua_wrap!(t, u32, en_metadata_file))?)?;
        globals
            .set("en_metadata_line", $lua.create_function($lua_wrap!(t, u32, en_metadata_line))?)?;
        globals.set(
            "en_metadata_target",
            $lua.create_function($lua_wrap!(t, u32, en_metadata_target))?,
        )?;
        globals.set(
            "en_metadata_module_path",
            $lua.create_function($lua_wrap!(t, u32, en_metadata_module_path))?,
        )?;
        globals.set("en_attrs", $lua.create_function($lua_wrap2!(t, u32, en_attrs))?)?;
        globals.set("en_attr_names", $lua.create_function($lua_wrap2!(t, u32, en_attr_names))?)?;
        globals
            .set("en_attr_values", $lua.create_function($lua_wrap2!(t, u32, en_attr_values))?)?;
        globals.set(
            "en_attr_by_idx",
            $lua.create_function($lua_wrap2!(t, (u32, usize), en_attr_by_idx))?,
        )?;
        globals.set(
            "en_attr_by_name",
            $lua.create_function($lua_wrap2!(t, (u32, String), en_attr_by_name))?,
        )?;
        globals.set(
            "en_attr_name",
            $lua.create_function($lua_wrap2!(t, (u32, usize), en_attr_name))?,
        )?;
        globals.set(
            "en_attr_value",
            $lua.create_function($lua_wrap2!(t, (u32, usize), en_attr_value))?,
        )?;
        globals.set("en_as_string", $lua.create_function($lua_wrap!(t, u32, en_as_string))?)?;
        let t = $trace.clone();
        globals
            .set("en_filter", $lua.create_function($lua_wrap!(t, (Vec<u32>, Table), en_filter))?)?;
        globals.set(
            "en_filter_range",
            $lua.create_function($lua_wrap!(t, (u32, u32, Table), en_filter_range))?,
        )?;
    };
}
pub fn setup_lua_on_arc_rwlock(
    lua: &mut Lua, range: RangeInclusive<u32>, trace: Arc<RwLock<TraceProvider>>,
    finder_cache: Rc<RefCell<HashMap<String, Finder<'static>>>>,
) -> Result<(), mlua::Error> {
    /// INPUT a Fn(impl LogProvider) -> Fn($arg) -> Result<T,E>
    /// OUTPUT a Fn(Arc<RwLock<Box<dyn LogProvider>>> -> Fn(Lua, $arg) -> mlua::Result<T>
    macro_rules! lua_wrap {
        ($trace_provider: expr, $arg: ty, $fn: expr) => {{
            let tp = $trace_provider.clone();
            move |_lua: &Lua, a: $arg| {
                let log = tp.read().unwrap();
                let adapter = DynAdapter(&**log);
                $fn(&adapter)(a).map_err(|x| x.into_lua_err())
            }
        }};
    }

    /// INPUT a Fn(impl LogProvider, Lua) -> Fn($arg) -> mlua::Result<T>
    /// OUTPUT a Fn(Arc<RwLock<Box<dyn LogProvider>>> -> Fn(Lua, $arg) -> mlua::Result<T>
    macro_rules! lua_wrap2 {
        ($trace_provider: expr, $arg: ty, $fn: expr) => {{
            let tp = $trace_provider.clone();
            move |lua: &Lua, a: $arg| {
                let log = tp.read().unwrap();
                let adapter = DynAdapter(&**log);
                $fn(&adapter, lua)(a)
            }
        }};
    }
    let t = trace.clone();
    lua.globals().set(
        "en_contains_anywhere",
        lua.create_function(move |_lua: &Lua, (id, needle): (u32, String)| {
            let log = t.read().unwrap();
            let adapter = DynAdapter(&**log);
            en_contains_anywhere(&adapter, finder_cache.clone())((id, needle)).map_err(to_lua_err)
        })?,
    )?;
    lua_setup_with_wrappers!(lua, trace, finder_cache, range, lua_wrap, lua_wrap2);
    Ok(())
}

pub fn setup_lua_no_lock(
    lua: &mut Lua, range: RangeInclusive<u32>, trace: Arc<TraceProvider>,
    finder_cache: Rc<RefCell<HashMap<String, Finder<'static>>>>,
) -> Result<(), mlua::Error> {
    /// INPUT a Fn(impl LogProvider) -> Fn($arg) -> Result<T,E>
    /// OUTPUT a Fn(Arc<RwLock<Box<dyn LogProvider>>> -> Fn(Lua, $arg) -> mlua::Result<T>
    macro_rules! lua_wrap {
        ($trace_provider: expr, $arg: ty, $fn: expr) => {{
            let tp = $trace_provider.clone();
            move |_lua: &Lua, a: $arg| {
                let adapter = DynAdapter(&**tp);
                $fn(&adapter)(a).map_err(|x| x.into_lua_err())
            }
        }};
    }

    /// INPUT a Fn(impl LogProvider, Lua) -> Fn($arg) -> mlua::Result<T>
    /// OUTPUT a Fn(Arc<RwLock<Box<dyn LogProvider>>> -> Fn(Lua, $arg) -> mlua::Result<T>
    macro_rules! lua_wrap2 {
        ($trace_provider: expr, $arg: ty, $fn: expr) => {{
            let tp = $trace_provider.clone();
            move |lua: &Lua, a: $arg| {
                let adapter = DynAdapter(&**tp);
                $fn(&adapter, lua)(a)
            }
        }};
    }
    let t = trace.clone();
    lua.globals().set(
        "en_contains_anywhere",
        lua.create_function(move |_lua: &Lua, (id, needle): (u32, String)| {
            let adapter = DynAdapter(&**t);
            en_contains_anywhere(&adapter, finder_cache.clone())((id, needle)).map_err(to_lua_err)
        })?,
    )?;
    lua_setup_with_wrappers!(lua, trace, finder_cache, range, lua_wrap, lua_wrap2);
    Ok(())
}

use std::{
    cell::RefCell,
    cmp::Ordering,
    collections::HashMap,
    error::Error,
    ops::RangeInclusive,
    rc::Rc,
    sync::{
        Arc, RwLock,
        atomic::{self, AtomicBool, AtomicUsize},
    },
    time::Instant,
};

use anyhow::bail;
use entrace_core::{
    EnValue, EnValueRef, LevelContainer, LogProvider, LogProviderError, LogProviderResult,
    MetadataRefContainer,
};
use memchr::memmem::Finder;
use mlua::{ExternalError, ExternalResult, IntoLua, Lua, Table, Value};
use roaring::RoaringBitmap;

use crate::{
    QueryError, TraceProvider,
    filtersets::{Filterset, Matcher, Predicate, PredicateId},
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

/// Handles the restricted subset of table copying we need.
/// Doesn't handle tables as keys.
fn deepcopy_table(lua: &Lua, table: Table) -> mlua::Result<Table> {
    let new_table = lua.create_table()?;

    for pair in table.pairs::<Value, Value>() {
        let (k, v) = pair?;
        let v_copy = match v {
            Value::Table(t2) => mlua::Value::Table(deepcopy_table(lua, t2)?),
            _ => v,
        };
        new_table.set(k, v_copy)?;
    }
    Ok(new_table)
}
fn to_lua_err(x: impl Error + Send + Sync + 'static) -> mlua::Error {
    x.into_lua_err()
}
#[doc = include_str!("../api-docs/en_pretty_table.md")]
pub fn en_pretty_table(table: Table) -> mlua::Result<String> {
    Ok(format!("{table:#?}"))
}
#[doc = include_str!("../api-docs/en_span_range.md")]
pub fn en_span_range(range: &RangeInclusive<u32>) -> mlua::Result<(u32, u32)> {
    Ok((*range.start(), *range.end()))
}
#[doc = include_str!("../api-docs/en_log.md")]
pub fn en_log(x: mlua::Value) -> mlua::Result<()> {
    println!("Lua log: {x:?}");
    Ok(())
}

#[doc = include_str!("../api-docs/en_children.md")]
pub fn en_children(tcc: &impl LogProvider) -> impl Fn(u32) -> Result<Vec<u32>, LogProviderError> {
    move |id: u32| tcc.children(id).map(|x| x.into())
}

#[doc = include_str!("../api-docs/en_child_cnt.md")]
pub fn en_child_cnt(tcc: &impl LogProvider) -> impl Fn(u32) -> Result<usize, LogProviderError> {
    move |id: u32| Ok(tcc.children(id)?.len())
}

#[doc = include_str!("../api-docs/en_span_cnt.md")]
pub fn en_span_cnt(tcc: &impl LogProvider) -> impl Fn(()) -> mlua::Result<usize> {
    move |_: ()| Ok(tcc.len())
}

#[doc = include_str!("../api-docs/en_metadata_table.md")]
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

#[doc = include_str!("../api-docs/en_metadata_name.md")]
pub fn en_metadata_name(
    tcc: &impl LogProvider,
) -> impl Fn(u32) -> Result<String, LogProviderError> {
    move |id: u32| Ok(tcc.meta(id)?.name.to_string())
}

#[doc = include_str!("../api-docs/en_metadata_level.md")]
pub fn en_metadata_level(tcc: &impl LogProvider) -> impl Fn(u32) -> LogProviderResult<u8> {
    move |id: u32| {
        let c = tcc.meta(id)?;
        Ok(level_to_u8(&c.level))
    }
}

#[doc = include_str!("../api-docs/en_metadata_file.md")]
pub fn en_metadata_file(
    tcc: &impl LogProvider,
) -> impl Fn(u32) -> LogProviderResult<Option<String>> {
    move |id: u32| {
        let c = tcc.meta(id)?;
        Ok(c.file.map(|x| x.to_owned()))
    }
}
#[doc = include_str!("../api-docs/en_metadata_line.md")]
pub fn en_metadata_line(tcc: &impl LogProvider) -> impl Fn(u32) -> mlua::Result<Option<u32>> {
    move |id: u32| {
        let c = tcc.meta(id).map_err(to_lua_err)?;
        Ok(c.line)
    }
}

#[doc = include_str!("../api-docs/en_metadata_target.md")]
pub fn en_metadata_target(tcc: &impl LogProvider) -> impl Fn(u32) -> LogProviderResult<String> {
    move |id: u32| {
        let c = tcc.meta(id)?;
        Ok(c.target.to_string())
    }
}

#[doc = include_str!("../api-docs/en_metadata_module_path.md")]
pub fn en_metadata_module_path(
    tcc: &impl LogProvider,
) -> impl Fn(u32) -> LogProviderResult<Option<String>> {
    move |id: u32| {
        let c = tcc.meta(id)?;
        Ok(c.module_path.map(|x| x.to_string()))
    }
}

#[doc = include_str!("../api-docs/en_attrs.md")]
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

#[doc = include_str!("../api-docs/en_attr_names.md")]
pub fn en_attr_names(
    tcc: &impl LogProvider, lua: &Lua,
) -> impl Fn(u32) -> mlua::Result<Vec<mlua::Value>> {
    move |id: u32| {
        let c = tcc.attrs(id).map_err(to_lua_err)?;
        let mut names = Vec::with_capacity(c.len());
        for (key, _) in c {
            names.push(key.into_lua(lua)?);
        }
        Ok(names)
    }
}

#[doc = include_str!("../api-docs/en_attr_values.md")]
pub fn en_attr_values(
    tcc: &impl LogProvider, lua: &Lua,
) -> impl Fn(u32) -> mlua::Result<Vec<mlua::Value>> {
    move |id: u32| {
        let c = tcc.attrs(id).map_err(to_lua_err)?;
        let mut values = Vec::with_capacity(c.len());
        for (_, value) in c {
            values.push(LuaValueRef(value).into_lua(lua)?);
        }
        Ok(values)
    }
}

#[doc = include_str!("../api-docs/en_attr_by_idx.md")]
pub fn en_attr_by_idx(
    tcc: &impl LogProvider, lua: &Lua,
) -> impl Fn((u32, usize)) -> mlua::Result<(mlua::String, mlua::Value)> {
    move |(id, idx): (u32, usize)| {
        let c = tcc.attrs(id).map_err(to_lua_err)?;
        let (k, v) = c.get(idx).ok_or_else(|| make_oob_error(idx as u32, c.len()))?;
        Ok((lua.create_string(k)?, LuaValueRefRef(v).into_lua(lua)?))
    }
}

#[doc = include_str!("../api-docs/en_attr_by_name.md")]
pub fn en_attr_by_name(
    tcc: &impl LogProvider, lua: &Lua,
) -> impl Fn((u32, String)) -> mlua::Result<mlua::Value> {
    move |(id, key): (u32, String)| {
        let attrs = tcc.attrs(id).map_err(to_lua_err)?;
        let attr = attrs.iter().find(|(k, _)| *k == key);
        if let Some(attr) = attr { LuaValueRefRef(&attr.1).into_lua(lua) } else { Ok(mlua::Nil) }
    }
}

#[doc = include_str!("../api-docs/en_attr_name.md")]
pub fn en_attr_name(
    tcc: &impl LogProvider, lua: &Lua,
) -> impl Fn((u32, usize)) -> mlua::Result<mlua::String> {
    move |(id, idx): (u32, usize)| {
        let c = tcc.attrs(id).map_err(to_lua_err)?;
        let (k, _) = c.get(idx).ok_or_else(|| make_oob_error(idx as u32, c.len()))?;
        lua.create_string(k)
    }
}

#[doc = include_str!("../api-docs/en_attr_value.md")]
pub fn en_attr_value(
    tcc: &impl LogProvider, lua: &Lua,
) -> impl Fn((u32, usize)) -> mlua::Result<mlua::Value> {
    move |(id, idx): (u32, usize)| {
        let c = tcc.attrs(id).map_err(to_lua_err)?;
        let (_, v) = c.get(idx).ok_or_else(|| make_oob_error(idx as u32, c.len()))?;
        LuaValueRefRef(v).into_lua(lua)
    }
}

#[doc = include_str!("../api-docs/en_as_string.md")]
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

#[doc = include_str!("../api-docs/en_contains_anywhere.md")]
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
/// Returns true if span_value R value
pub fn values_match(
    comparator: std::cmp::Ordering, span_value: &EnValueRef, value: &EnValue,
) -> bool {
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
pub fn span_matches_filter(
    tcc: &impl LogProvider, id: u32, target: &str, target_is_meta: bool, relation: Ordering,
    en_value: &EnValue,
) -> bool {
    if target_is_meta {
        let meta = tcc.meta(id).unwrap();
        meta_matches(&meta, target, relation, en_value).map_err(|x| x.into_lua_err()).unwrap()
    } else {
        let attrs = tcc.attrs(id).unwrap();
        let Some((_name, target_here)) = attrs.iter().find(|(name, _)| *name == target) else {
            return false;
        };
        values_match(relation, target_here, en_value)
    }
}

// =========================================FILTERSET API FOR LUA=========================================
// A filterset looks like:
//   type: "filterset"
//   root: 1
//   items: {
//     { type = "prim_list"; value = [1,2,3];},
//     { type = "rel_dnf", clauses = {{ target = "", relation = "", value = ""}}, src = 0},
//   }
//
//   Valid item types are: "prim_list", "prim_range", "rel_dnf",
//   "intersect", "union", "invert"

// en_filterset_from_list()
//  input: list of ids
//  outputs: a table with
//    type: "filterset"
//    root: 0
//    items: {
//      { type = "prim_list"; value = the list}
//    }
#[doc = include_str!("../api-docs/en_filterset_from_list.md")]
pub fn en_filterset_from_list(lua: &Lua, t: Table) -> mlua::Result<Table> {
    let fs = lua.create_table()?;
    fs.set("type", "filterset")?;
    fs.set("root", 0)?;

    let item = lua.create_table()?;
    item.set("type", "prim_list")?;
    item.set("value", t)?;

    let items = lua.create_table()?;
    items.push(item)?;
    fs.set("items", items)?;
    Ok(fs)
}

// en_filterset_from_range()
//  input: start, end
//  outputs: a table with
//    type: "filterset"
//    root: 0
//    items: {
//      { type = "prim_range"; start = start, end=end}
//    }
#[doc = include_str!("../api-docs/en_filterset_from_range.md")]
pub fn en_filterset_from_range(lua: &Lua, (start, end): (usize, usize)) -> mlua::Result<Table> {
    let fs = lua.create_table()?;
    fs.set("type", "filterset")?;
    fs.set("root", 0)?;

    let item = lua.create_table()?;
    item.set("type", "prim_range")?;
    item.set("start", start)?;
    item.set("end", end)?;

    let items = lua.create_table()?;
    items.push(item)?;
    fs.set("items", items)?;
    Ok(fs)
}

// en_filter()
// input:
//   filter: table with
//     target: name of variable eg. "message" or "meta.filename"
//     relation: a relation, one of "EQ", "LT", "GT"
//     value: a constant to compare with
//   src: filterset
// outputs: { type = "filterset", root = 1, items = { src = 0, {type = "rel_dnf", src = 0, clauses = {{ target, relation, value}} }}},
#[doc = include_str!("../api-docs/en_filter.md")]
pub fn en_filter(lua: &Lua, (filter, src): (Table, Table)) -> mlua::Result<Table> {
    let old_items: Table = src.get("items")?;
    let items_len = old_items.len()?;
    let new_items = deepcopy_table(lua, old_items)?;
    let old_root: usize = src.get("root")?;

    let dnf_filter = lua.create_table()?;
    dnf_filter.set("type", "rel_dnf")?;
    dnf_filter.set("src", old_root)?;
    let clauses_outer = lua.create_table()?;
    let clauses_inner = lua.create_table()?;
    let pred2 = deepcopy_table(lua, filter)?;

    clauses_inner.push(pred2)?;
    clauses_outer.push(clauses_inner)?;
    dnf_filter.set("clauses", clauses_outer)?;
    new_items.push(dnf_filter)?;

    let fs = lua.create_table()?;
    fs.set("type", "filterset")?;
    fs.set("root", items_len)?;
    fs.set("items", new_items)?;
    Ok(fs)
}

/// Helper used by [en_filterset_union] and [en_filterset_intersect] to fix up the source pointers
/// in item lists when concatenating multiple items lists
fn increment_item_source(amount: i64, item: &Table) -> mlua::Result<()> {
    if let Ok(mlua::Value::Integer(q)) = item.get("src") {
        item.set("src", q + amount)?;
    }
    if let Ok(mlua::Value::Table(srcs)) = item.get("srcs") {
        let len = srcs.len()?;
        for x in 1..=len {
            if let Ok(mlua::Value::Integer(q)) = srcs.get(x) {
                srcs.set(x, q + amount)?;
            }
        }
    }
    Ok(())
}

fn concat_items_lists(lua: &Lua, filters: Table) -> mlua::Result<(Table, Vec<i64>)> {
    let all_items = lua.create_table()?;
    let filter_cnt = filters.len()?;
    let mut additional_len_before = 0;
    let mut srcs = vec![];
    for i in 1..=filter_cnt {
        let filter: Table = filters.get(i)?;
        let items: Table = filter.get("items")?;
        let item_cnt = items.len()?;
        for j in 1..=item_cnt {
            let item: Table = items.get(j)?;
            let item = deepcopy_table(lua, item)?;
            increment_item_source(additional_len_before, &item)?;
            all_items.push(item)?;
        }
        srcs.push(item_cnt - 1 + additional_len_before);
        additional_len_before += item_cnt;
    }

    Ok((all_items, srcs))
}

//  en_filterset_union()
//  input:
//    filters: a list of filtersets, e. g
//    {
//      { type: "filterset",
//        root: 1,
//        items: {
//          { type = "prim_list", value = {1,2,3}},
//          { type = "rel", target = "a", relation = "EQ", value = "1", src = 0 },
//        }
//      }
//      { type: "filterset",
//        root: 1,
//        items: {
//          {type: "prim_list", value = {1,2,3} },
//          {type: "rel", target = "b", relation = "EQ", value = "1", src = 0},
//        }
//      }
//    }
//  outputs: a filterset that matches an item if it is in any input filterset.
//  This does NOT deduplicate any items, eg. for the given inputs, the result would be as follows.
//  Note that en_materialize() MAY deduplicate, but there is no guarantee it will.
//  { type: "filterset",
//    root: 4,
//    items: {
//      { type = "prim_list", value = {1,2,3}},
//      { type = "rel", target = "a", relation = "EQ", value = "1", src = 0 },
//      { type: "prim_list", value = {1,2,3}},
//      { type: "rel", target = "b", relation = "EQ", value = "1", src = 2 },
//      { type: "union", srcs = { 1, 3 }}
//  }
#[doc = include_str!("../api-docs/en_filterset_union.md")]
pub fn en_filterset_union(lua: &Lua, filters: Table) -> mlua::Result<Table> {
    let fs = lua.create_table()?;
    fs.set("type", "filterset")?;
    let (all_items, srcs) = concat_items_lists(lua, filters)?;
    let union = lua.create_table()?;
    union.set("type", "union")?;
    union.set("srcs", srcs)?;
    all_items.push(union)?;
    fs.set("root", all_items.len()? - 1)?;
    fs.set("items", all_items)?;
    Ok(fs)
}

// en_filterset_intersect()
// input:
//   filters: a list of filtersets, e. g
//   {
//     { type: "filterset",
//       root: 1,
//       items: {
//         { type = "prim_list", value = {1,2,3}},
//         { type = "rel", target = "a", relation = "EQ", value = "1", src = 0 },
//       }
//     }
//     { type: "filterset",
//       root: 1,
//       items: {
//         {type: "prim_list", value = {1,2,3} },
//         {type: "rel", target = "b", relation = "EQ", value = "1", src = 0},
//       }
//     }
//   }
// outputs: a filterset that matches an item if it is in all input filtersets.
// This does NOT deduplicate any items, eg. for the given inputs, the result would be as follows.
// Note that en_materialize() MAY deduplicate, but there is no guarantee it will. (it currently
// doesn't, because an acyclic graph is required for evauator correctness, this might change).
// { type: "filterset",
//   root: 4,
//   items: {
//     { type = "prim_list", value = {1,2,3}},
//     { type = "rel", target = "a", relation = "EQ", value = "1", src = 0 },
//     { type: "prim_list", value = {1,2,3}},
//     { type: "rel", target = "b", relation = "EQ", value = "1", src = 2 },
//     { type: "intersect", srcs = { 1, 3 }}
// }
#[doc = include_str!("../api-docs/en_filterset_intersect.md")]
pub fn en_filterset_intersect(lua: &Lua, filters: Table) -> mlua::Result<Table> {
    let fs = lua.create_table()?;
    fs.set("type", "filterset")?;
    let (all_items, srcs) = concat_items_lists(lua, filters)?;

    let union = lua.create_table()?;
    union.set("type", "intersect")?;
    union.set("srcs", srcs)?;
    all_items.push(union)?;
    fs.set("root", all_items.len()? - 1)?;
    fs.set("items", all_items)?;
    Ok(fs)
}

//  en_filterset_dnf()
//  input:
//    filters: a list of list of filter descriptions, which is interpreted as a DNF clause list.
//    (this example would be (a=1 AND c=0) OR (b=1)
//    {
//      {
//        { target = "a", relation = "EQ", value = "1", src = 0 },
//        { target = "c", relation = "EQ", value = "0", src = 0 },
//      }
//      {
//        { target = "b", relation = "EQ", value = "1", src = 0},
//      }
//    }
//    source: a filterset
//  outputs: a filterset that matches an item if satisfies either of the AND clauses
//  { type: "filterset",
//    root: 1,
//    items: {
//      { type = "prim_list", value = {1,2,3}},
//      { type = "rel_dnf", src = 0,
//        clauses = {
//          {
//            { target = "a", relation = "EQ", value = "1", src = 0 },
//            { target = "c", relation = "EQ", value = "0", src = 0 },
//          }
//          {
//            { target = "b", relation = "EQ", value = "1", src = 0},
//          }
//        }
//      }
//  }
#[doc = include_str!("../api-docs/en_filterset_dnf.md")]
pub fn en_filterset_dnf(lua: &Lua, (clauses, src): (Table, Table)) -> mlua::Result<Table> {
    let new_fs = deepcopy_table(lua, src)?;
    let old_root: usize = new_fs.get("root")?;
    let items: Table = new_fs.get("items")?;

    let dnf_item = lua.create_table()?;
    dnf_item.set("type", "rel_dnf")?;
    dnf_item.set("clauses", deepcopy_table(lua, clauses)?)?;
    dnf_item.set("src", old_root)?;
    items.push(dnf_item)?;
    new_fs.set("root", items.len()? - 1)?;
    Ok(new_fs)
}

// en_filterset_not()
// input: filterset
// outputs: a filterset that matches an item exactly if it is not in the filterset.
#[doc = include_str!("../api-docs/en_filterset_not.md")]
pub fn en_filterset_not(lua: &Lua, filterset: Table) -> mlua::Result<Table> {
    let new_fs = deepcopy_table(lua, filterset)?;
    let not = lua.create_table()?;
    not.set("type", "invert")?;
    let new_items: Table = new_fs.get("items")?;
    not.set("src", new_fs.len()? - 1)?;
    new_items.push(not)?;
    new_fs.set("root", new_items.len()? - 1)?;
    Ok(new_fs)
}

/// Creates a Predicate from a Table that has keys "target", "relation", "value"
fn parse_predicate(t: &Table) -> mlua::Result<Predicate<EnValue>> {
    //     { type = "rel", target = "", relation = "", value = "", src = 0 },
    let attr: String = t.get("target")?;
    let relation: String = t.get("relation")?;
    let rel = match relation.as_str() {
        "GT" => Ordering::Greater,
        "LT" => Ordering::Less,
        "EQ" => Ordering::Equal,
        x => return Err(anyhow::anyhow!("Bad filter relation {x}").into_lua_err()),
    };

    let value: mlua::Value = t.get("value")?;
    let en_value = match value {
        Value::Boolean(f) => EnValue::Bool(f),
        Value::Integer(k) => EnValue::I64(k),
        Value::Number(z) => EnValue::Float(z),
        Value::String(ref q) => EnValue::String(q.to_string_lossy()),
        x => {
            return Err(anyhow::anyhow!("Cannot convert value {x:?} to EnValue").into_lua_err());
        }
    };
    Ok(Predicate { attr, rel, constant: en_value })
}
fn item_to_filterset(
    item: &Table, mut add_predicate: impl FnMut(Predicate<EnValue>) -> PredicateId,
) -> mlua::Result<Filterset> {
    let ty: String = item.get("type")?;
    match ty.as_str() {
        "prim_list" => {
            let value: Vec<u32> = item.get("value")?;
            Ok(Filterset::Primitive(RoaringBitmap::from_iter(value)))
        }
        "prim_range" => {
            let start: u32 = item.get("start")?;
            let end: u32 = item.get("end")?;
            let bm = RoaringBitmap::from_sorted_iter(start..=end).into_lua_err()?;
            Ok(Filterset::Primitive(bm))
        }
        "rel_dnf" => {
            //     { type = "rel_dnf",
            //       clauses = {
            //         {
            //           { target = "", relation = "GT", value = ""},
            //           { target = "", relation = "LT", value = ""},
            //         },
            //       },
            //       src = 0,
            //     }
            let clauses: Table = item.get("clauses")?;
            let clauses_len = clauses.len()? as usize;
            let mut new_clauses = Vec::with_capacity(clauses_len);
            for i in 1..=clauses_len {
                let anded_clauses: Table = clauses.get(i)?;
                let anded_clauses_len = anded_clauses.len()? as usize;
                let mut new_clause = Vec::with_capacity(anded_clauses_len);
                for j in 1..=anded_clauses_len {
                    let predicate = anded_clauses.get(j)?;
                    new_clause.push(add_predicate(parse_predicate(&predicate)?));
                }
                new_clauses.push(new_clause);
            }
            let src: usize = item.get("src")?;
            Ok(Filterset::RelDnf(new_clauses, src))
        }
        "intersect" => {
            //     { type: "intersect", srcs = { 1, 3 }}
            Ok(Filterset::And(item.get("srcs")?))
        }
        "union" => Ok(Filterset::And(item.get("srcs")?)),
        "invert" => Ok(Filterset::Not(item.get("src")?)),
        x => Err(anyhow::anyhow!("Unknown filterset item type {x}").into_lua_err()),
    }
}

pub struct EnMatcher<'a, L: LogProvider> {
    pub log: &'a L,
}
pub struct EnPredicate<'a> {
    target: &'a str,
    target_is_meta: bool,
    rel: Ordering,
    con: &'a EnValue,
}
pub fn predicate_to_en_predicate<'a>(p: &'a Predicate<EnValue>) -> EnPredicate<'a> {
    let Predicate { attr, rel, constant: con } = p;
    let mut target = attr.as_str();
    let mut target_is_meta = false;
    if let Some(stripped) = target.strip_prefix("meta.") {
        target = stripped;
        target_is_meta = true;
    }
    EnPredicate { target, target_is_meta, rel: *rel, con }
}
impl<L: LogProvider> Matcher<EnValue> for EnMatcher<'_, L> {
    fn subset_matching(
        &self, predicate: &Predicate<EnValue>, input: &RoaringBitmap,
    ) -> RoaringBitmap {
        let mut res = input.clone();
        let EnPredicate { target, target_is_meta, rel, con } = predicate_to_en_predicate(predicate);
        for id in input {
            let matches_here = span_matches_filter(self.log, id, target, target_is_meta, rel, con);
            if !matches_here {
                res.remove(id);
            }
        }
        res
    }

    fn subset_matching_dnf<'a, O, I>(&self, clauses: O, input: &RoaringBitmap) -> RoaringBitmap
    where
        O: Iterator<Item = I>,
        I: Iterator<Item = &'a Predicate<EnValue>>,
        EnValue: 'a,
    {
        let mut res = RoaringBitmap::new();
        let predicates_prepared: Vec<Vec<EnPredicate>> =
            clauses.map(|x| x.map(|y| predicate_to_en_predicate(y)).collect()).collect();
        'outer: for id in input {
            for anded_clause in predicates_prepared.iter() {
                let mut matches_in_and = true;
                for predicate in anded_clause {
                    let EnPredicate { target, target_is_meta, rel, con } = predicate;
                    matches_in_and &=
                        span_matches_filter(self.log, id, target, *target_is_meta, *rel, con)
                }
                // OR join -> matches in one anded clause means the whole thing matches
                if matches_in_and {
                    res.insert(id);
                    continue 'outer;
                }
            }
        }
        res
    }
}
#[doc = include_str!("../api-docs/en_filterset_materialize.md")]
pub fn en_filterset_materialize(
    log: &impl LogProvider, lua: &Lua,
) -> impl Fn(Table) -> mlua::Result<Table> {
    |filterset: Table| {
        let mut evaluator = crate::filtersets::Evaluator::new();
        let root: usize = filterset.get("root")?;
        let items: Table = filterset.get("items")?;
        let item_cnt = items.len()?;

        for i in 1..=item_cnt {
            let item: Table = items.get(i)?;
            let fs = item_to_filterset(&item, |p| evaluator.new_predicate(p))?;
            evaluator.new_filterset(fs);
        }
        let nstart = Instant::now();
        evaluator.normalize(root);
        eprintln!("normalization took {:?}", nstart.elapsed());
        let matcher = EnMatcher { log };
        evaluator.materialize(&matcher, root);

        let tstart = Instant::now();
        let table = lua.create_sequence_from(evaluator.results[&root].iter())?;
        eprintln!("allocating results table to lua took {:?}", tstart.elapsed());
        Ok(table)
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

pub struct JoinCtx {
    is_joining: AtomicBool,
    threads_joined: AtomicUsize,
    thread_count: usize,
    results: RwLock<Vec<u32>>,
}
impl JoinCtx {
    pub fn from_thread_count(thread_count: usize) -> Self {
        JoinCtx {
            is_joining: false.into(),
            threads_joined: 0.into(),
            thread_count,
            results: RwLock::new(vec![]),
        }
    }
}

#[doc = include_str!("../api-docs/en_join.md")]
pub fn en_join(joinable: Arc<JoinCtx>) -> impl Fn(Table) -> LogProviderResult<Vec<u32>> {
    move |results: Table| {
        joinable.is_joining.store(true, atomic::Ordering::Release);
        let joined_cnt = joinable.threads_joined.fetch_add(1, atomic::Ordering::AcqRel) + 1;
        if joined_cnt < joinable.thread_count {
            // this is not the last thread  to finish, so we shut down
            let mut all_results_w = joinable.results.write().unwrap();
            // this is what FromLua<Vec<u32>> would do too, but we do it directly on the table to
            // avoid copying twice.
            all_results_w.extend(results.sequence_values::<u32>().filter_map(|x| x.ok()));
            drop(all_results_w);
            return Err(LogProviderError::JoinShutdown);
        }
        // last thread, this one keeps running

        // reset join state
        joinable.is_joining.store(false, atomic::Ordering::Release);
        joinable.threads_joined.store(0, atomic::Ordering::Release);
        let mut all_results = joinable.results.write().unwrap();
        let mut old_results = std::mem::take(&mut *all_results);
        old_results.extend(results.sequence_values::<u32>().filter_map(|x| x.ok()));
        Ok(old_results)
    }
}

macro_rules! lua_setup_with_wrappers {
    ($lua: expr, $trace: expr, $finder_cache: expr, $join_ctx: expr, $range: expr, $lua_wrap: ident, $lua_wrap2: ident) => {
        let globals = $lua.globals();
        let en_range = $lua.create_function(move |_state, _: ()| en_span_range(&$range));
        globals.set("en_span_range", en_range?)?;
        globals.set("en_log", $lua.create_function(move |_, x| en_log(x))?)?;
        globals.set("en_pretty_table", $lua.create_function(move |_, t| en_pretty_table(t))?)?;
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

        globals.set("en_filterset_from_list", $lua.create_function(en_filterset_from_list)?)?;
        globals.set("en_filterset_from_range", $lua.create_function(en_filterset_from_range)?)?;
        globals.set("en_filter", $lua.create_function(en_filter)?)?;
        globals.set("en_filterset_union", $lua.create_function(en_filterset_union)?)?;
        globals.set("en_filterset_intersect", $lua.create_function(en_filterset_intersect)?)?;
        globals.set("en_filterset_dnf", $lua.create_function(en_filterset_dnf)?)?;
        globals.set("en_filterset_not", $lua.create_function(en_filterset_not)?)?;
        globals.set(
            "en_filterset_materialize",
            $lua.create_function($lua_wrap2!(t, Table, en_filterset_materialize))?,
        )?;
        let join_fn = en_join($join_ctx);
        globals.set(
            "en_join",
            $lua.create_function(move |_: &Lua, results: Table| {
                join_fn(results).map_err(to_lua_err)
            })?,
        )?;
    };
}
pub fn setup_lua_on_arc_rwlock(
    lua: &mut Lua, range: RangeInclusive<u32>, trace: Arc<RwLock<TraceProvider>>,
    finder_cache: Rc<RefCell<HashMap<String, Finder<'static>>>>, join_ctx: Arc<JoinCtx>,
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

    lua_setup_with_wrappers!(lua, trace, finder_cache, join_ctx, range, lua_wrap, lua_wrap2);
    Ok(())
}

pub fn setup_lua_no_lock(
    lua: &mut Lua, range: RangeInclusive<u32>, trace: Arc<TraceProvider>,
    finder_cache: Rc<RefCell<HashMap<String, Finder<'static>>>>, join_ctx: Arc<JoinCtx>,
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
    lua_setup_with_wrappers!(lua, trace, finder_cache, join_ctx, range, lua_wrap, lua_wrap2);
    Ok(())
}

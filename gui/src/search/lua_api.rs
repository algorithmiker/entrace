use std::{
    cell::RefCell,
    collections::HashMap,
    ops::RangeInclusive,
    rc::Rc,
    sync::{Arc, RwLock},
};

use entrace_core::{EnValueRef, LogProviderError, MetadataRefContainer};
use memchr::memmem::Finder;
use mlua::{IntoLua, Lua, Table, Value};

use crate::{
    LevelRepr, TraceProvider,
    search::{LuaValueRef, LuaValueRefRef, QueryError},
};

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
        table.set("level", level.index())?;
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
        Ok(level.index())
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

    Ok(())
}

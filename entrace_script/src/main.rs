use std::{cell::RefCell, collections::HashMap, path::PathBuf, rc::Rc, sync::Arc};

use clap::Parser;

#[derive(Parser)]
#[command(version, about, long_about = "Run a Lua script with access to the entrace Lua API")]
struct Args {
    #[arg(short, long, value_name = "FILE")]
    lua_file: PathBuf,
    #[arg(short, long, value_name = "FILE")]
    trace_file: PathBuf,
}

fn main() -> anyhow::Result<()> {
    let Args { lua_file, trace_file } = Args::parse();
    let trace =
        unsafe { entrace_core::load_trace(trace_file, entrace_core::LoadConfig::default()) }?;
    let trace_arc = Arc::new(trace);
    let trace_len = trace_arc.len().saturating_sub(1) as u32;

    let mut lua = mlua::Lua::new();
    let finder_cache = Rc::new(RefCell::new(HashMap::new()));
    entrace_query::lua_api::setup_lua_no_lock(&mut lua, 0..=trace_len, trace_arc, finder_cache)?;

    let lua_file_contents = std::fs::read_to_string(&lua_file)?;
    lua.load(lua_file_contents).set_name(format!("@{}", lua_file.display())).exec()?;
    Ok(())
}

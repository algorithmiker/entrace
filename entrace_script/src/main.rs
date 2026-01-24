use std::{cell::RefCell, collections::HashMap, path::PathBuf, rc::Rc, sync::Arc};

use clap::Parser;
use entrace_query::lua_api::{JoinCtx, LuaEvalState};

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
    let mut lua = mlua::Lua::new();
    let finder_cache = Rc::new(RefCell::new(HashMap::new()));
    let join_ctx_arc = Arc::new(JoinCtx::from_thread_count(1));
    let state =
        LuaEvalState::new(join_ctx_arc, 0..=trace_arc.len().saturating_sub(1) as u32, finder_cache);
    entrace_query::lua_api::setup_lua_no_lock(&mut lua, trace_arc, state)?;

    let lua_file_contents = std::fs::read_to_string(&lua_file)?;
    lua.load(lua_file_contents).set_name(format!("@{}", lua_file.display())).exec()?;
    Ok(())
}

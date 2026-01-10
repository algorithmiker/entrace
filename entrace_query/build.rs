use std::fmt::Write as fmt_write;
use std::fs::OpenOptions;
use std::io::Write as io_write;
use std::path::PathBuf;
use std::{fs, path::Path};
use syn::{Item, Visibility};

#[derive(Debug)]
#[allow(unused)]
pub struct Function {
    name: String,
    docs: String,
}
const LUA_API_PATH: &str = "src/lua_api.rs";
/// This build.rs primarily does two things:
/// 1. it exports lua api docs as static strings in a submodule of this crate, to allow displaying docs in the GUI
/// 2. it validates that these docs are correct, and that every public en_* function has a doc file.
fn main() {
    println!("cargo::rerun-if-changed={LUA_API_PATH}");
    println!("cargo::rerun-if-changed=api-docs");
    let content = fs::read_to_string(LUA_API_PATH).expect("Unable to read src/lua_api.rs");
    let file = syn::parse_file(&content).expect("Unable to parse src/lua_api.rs");
    let out_dir = std::env::var_os("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("lua_api_docs.rs");

    let mut fns = Vec::with_capacity(20);
    let folder_path = Path::new("api-docs");
    for name in api_fn_names(&file.items) {
        let file_path = folder_path.join(format!("{name}.md"));
        let Ok(doc) = std::fs::read_to_string(&file_path) else {
            println!(
                "cargo::error=Failed to read docs at {}, maybe it doesn't exist?",
                file_path.display()
            );
            return;
        };
        let doc = doc.trim().to_string();
        if let Err(msg) = validate_docs(&doc) {
            println!("cargo::error=Validation failed for {}: {msg}", file_path.display());
        };
        fns.push(Function { name: name.clone(), docs: doc });
    }
    fns.sort_unstable_by(|x, y| x.name.cmp(&y.name));
    export_to_file(&fns, &dest_path);
}
pub fn export_to_file(fns: &[Function], out_path: &PathBuf) {
    let mut buf = String::from(
        "pub struct Function {
pub name: &'static str,
pub docs: &'static str,
}\n",
    );
    write!(buf, "pub const LUA_API_DOCS: [Function; {}] = {fns:#?};", fns.len()).unwrap();
    let fn_names: Vec<&str> = fns.iter().map(|x| x.name.as_str()).collect();
    write!(buf, "pub const LUA_FN_NAMES: [&str; {}] = {fn_names:#?};", fns.len()).unwrap();

    let mut outfile = OpenOptions::new()
        .truncate(true)
        .create(true)
        .write(true)
        .open(out_path)
        .expect("Failed to open outfile");
    outfile.write_all(buf.as_bytes()).expect("failed to write output");
    println!("cargo:warning=wrote {}", out_path.display());
}
pub fn api_fn_names(items: &[Item]) -> impl Iterator<Item = String> {
    items.iter().filter_map(|x| -> Option<String> {
        if let Item::Fn(func) = x
            && let Visibility::Public(_) = func.vis
            && let func_name = func.sig.ident.to_string()
            && func_name.starts_with("en_")
        {
            Some(func_name)
        } else {
            None
        }
    })
}
// TODO: this.
// ideally the format would be something like
//
// en_function_name
// Description of en_function_name.
//
// ## INPUT
//   id: u32
//   blah: string
// ## OUTPUT
//   blah
// ## EXAMPLE
// local bar = en_function_name(1, "foo")
pub fn validate_docs(inp: &str) -> Result<(), String> {
    let (mut input_fnd, mut output_fnd, mut example_fnd) = (false, false, false);
    for line in inp.lines() {
        if line.starts_with("## INPUT") {
            input_fnd = true;
        }
        if line.starts_with("## OUTPUT") {
            output_fnd = true;
        }
        if line.starts_with("## EXAMPLE") {
            example_fnd = true;
        }
    }
    if input_fnd && output_fnd && example_fnd {
        return Ok(());
    }
    let mut err = String::from("Expected to find subheading(s): ");
    for (found, s) in [(input_fnd, "INPUT"), (output_fnd, "OUTPUT"), (example_fnd, "EXAMPLE")] {
        if !found {
            err.push_str(s);
            err.push(',');
        }
    }
    err.pop();
    Err(err)
}

//fn find_subheading(lines: &[&str]) -> Option<usize>{
//    for line in lines {
//        if line.starts_with("##") {
//            return ;
//        }
//    }
//}

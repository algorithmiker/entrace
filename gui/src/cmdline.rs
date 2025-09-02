#[derive(clap::Parser)]
pub struct Cmdline {
    pub file_path: Option<String>,
    #[arg(long = "option", value_name = "CONFIG_DECLARATION")]
    pub option_overrides: Vec<String>,
}

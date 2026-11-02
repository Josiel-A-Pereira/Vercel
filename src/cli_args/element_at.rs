use clap::Parser;
use serde::Deserialize;

fn default_max_depth() -> u8 {
    10
}

#[derive(Parser, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ElementAtArgs {
    #[command(flatten)]
    #[serde(flatten)]
    pub scope: super::WindowScope,
    #[arg(
        long,
        help = "Screen coordinates as x,y; write a negative value with = (--xy=-100,50)"
    )]
    pub xy: String,
    #[arg(long, default_value = "10", help = "Maximum tree depth (default 10)")]
    #[serde(default = "default_max_depth")]
    pub max_depth: u8,
    #[arg(
        long = "timeout-ms",
        value_name = "MS",
        help = "Snapshot and hit-test deadline in milliseconds (default 3000)"
    )]
    #[serde(default)]
    pub timeout_ms: Option<u64>,
}

mod config;
mod event;
mod group;
mod hash;
mod runner;
mod stats;
mod vdf;

use crate::config::Config;
use crate::runner::Runner;
use std::env;
use std::fs;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = env::args().collect();
    let config_path = args.get(1).map(String::as_str).unwrap_or("run.toml");
    let config_text = fs::read_to_string(config_path)?;
    let config: Config = toml::from_str(&config_text)?;
    let runner = Runner::new(config);
    runner.run()
}

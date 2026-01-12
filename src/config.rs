use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct Config {
    pub tasks: TasksConfig,
    pub vdf: VdfConfig,
    pub runner: RunnerConfig,
    pub output: OutputConfig,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct TasksConfig {
    pub ticks: usize,
    pub warmup: usize,
    pub mode: InputMode,
    pub seed: u64,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct VdfConfig {
    pub n_bits: usize,
    pub t: u64,
    pub k: usize,
    pub proof_algo: ProofAlgo,
    pub kappa: Option<usize>,
    pub target_duration_ns: Option<u128>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct RunnerConfig {
    pub cooldown_ms: u64,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct OutputConfig {
    pub events_jsonl_path: String,
    pub summary_json_path: String,
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy)]
#[serde(rename_all = "kebab-case")]
pub enum InputMode {
    FixedInput,
    RandomInput,
    Chained,
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy)]
#[serde(rename_all = "kebab-case")]
pub enum ProofAlgo {
    Alg4,
    Alg5,
}

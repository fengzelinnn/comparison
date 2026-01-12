use crate::config::{Config, InputMode, ProofAlgo};
use crate::event::{TimeUnit, TimeUnitEvent};
use crate::group::RsaGroup;
use crate::stats::summarize;
use crate::vdf::{eval, verify};
use num_bigint::BigUint;
use rand::RngCore;
use rand::SeedableRng;
use rand_chacha::ChaCha20Rng;
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub struct Runner {
    config: Config,
}

impl Runner {
    pub fn new(config: Config) -> Self {
        Self { config }
    }

    pub fn run(&self) -> anyhow::Result<()> {
        if let ProofAlgo::Alg5 = self.config.vdf.proof_algo {
            anyhow::bail!("Alg5 is not implemented in this experiment");
        }
        let mut rng = ChaCha20Rng::seed_from_u64(self.config.tasks.seed);
        let modulus = generate_rsa_modulus(&mut rng, self.config.vdf.n_bits);
        let group = RsaGroup::new(modulus);
        let mut x = random_bytes(&mut rng, 32);
        let run_id = build_run_id(&self.config)?;
        let run_start = Instant::now();
        let mut durations = Vec::with_capacity(self.config.tasks.ticks);

        let mut writer = jsonl_writer(&self.config.output.events_jsonl_path)?;

        for tick_index in 0..self.config.tasks.ticks {
            match self.config.tasks.mode {
                InputMode::FixedInput => {}
                InputMode::RandomInput => {
                    x = random_bytes(&mut rng, 32);
                }
                InputMode::Chained => {}
            }
            let start = Instant::now();
            let output = eval(&group, &x, self.config.vdf.t, self.config.vdf.k);
            let duration = start.elapsed();
            let verify_start = Instant::now();
            let ok = verify(
                &group,
                &output.g,
                &output.y,
                &output.proof,
                self.config.vdf.t,
                self.config.vdf.k,
            );
            let verify_duration = verify_start.elapsed();
            let err_msg = if ok { "" } else { "verify_failed" };
            let start_offset = run_start.elapsed().saturating_sub(duration);
            let unit = TimeUnit {
                unit_id: tick_index as u64,
                unit_type: "vdf_tick".to_string(),
                target_duration_ns: self.config.vdf.target_duration_ns,
                start_ts_ns: start_offset.as_nanos(),
                end_ts_ns: start_offset.as_nanos() + duration.as_nanos(),
                duration_ns: duration.as_nanos(),
                work_amount: Some(self.config.vdf.t),
                proof_size_bytes: Some(output.proof.to_bytes_be().len()),
                verify_time_ns: Some(verify_duration.as_nanos()),
                metadata: vdf_metadata(
                    self.config.tasks.mode,
                    self.config.vdf.t,
                    self.config.vdf.k,
                    self.config.vdf.proof_algo,
                    ok,
                    err_msg,
                ),
            };
            let event = TimeUnitEvent {
                run_id: run_id.clone(),
                unit,
            };
            write_jsonl(&mut writer, &event)?;
            if tick_index >= self.config.tasks.warmup {
                durations.push(duration.as_nanos());
            }
            if let InputMode::Chained = self.config.tasks.mode {
                x = next_challenge(&x, &output.y, tick_index as u64);
            }
            if self.config.runner.cooldown_ms > 0 {
                std::thread::sleep(Duration::from_millis(self.config.runner.cooldown_ms));
            }
        }
        writer.flush()?;

        if let Some(summary) = summarize(&durations, self.config.vdf.target_duration_ns) {
            let report = SummaryReport {
                run_id,
                unit_type: "vdf_tick".to_string(),
                target_duration_ns: self.config.vdf.target_duration_ns,
                summary,
            };
            write_summary(&self.config.output.summary_json_path, &report)?;
            println!("wrote_summary: {}", self.config.output.summary_json_path);
        }
        Ok(())
    }
}

fn generate_rsa_modulus(rng: &mut ChaCha20Rng, bits: usize) -> BigUint {
    let half = bits / 2;
    let p = random_prime(rng, half);
    let q = random_prime(rng, bits - half);
    &p * &q
}

fn random_prime(rng: &mut ChaCha20Rng, bits: usize) -> BigUint {
    loop {
        let mut bytes = vec![0u8; (bits + 7) / 8];
        rng.fill_bytes(&mut bytes);
        bytes[0] |= 0b1000_0000;
        let last = bytes.len() - 1;
        bytes[last] |= 1;
        let candidate = BigUint::from_bytes_be(&bytes);
        let mut seed = [0u8; 32];
        rng.fill_bytes(&mut seed);
        if crate::hash::is_probable_prime(&candidate, 16, &seed) {
            return candidate;
        }
    }
}

fn random_bytes(rng: &mut ChaCha20Rng, len: usize) -> Vec<u8> {
    let mut bytes = vec![0u8; len];
    rng.fill_bytes(&mut bytes);
    bytes
}

fn next_challenge(x: &[u8], y: &BigUint, tick_index: u64) -> Vec<u8> {
    let mut hasher = Sha256::new();
    hasher.update(x);
    hasher.update(y.to_bytes_be());
    hasher.update(tick_index.to_be_bytes());
    hasher.finalize().to_vec()
}

fn build_run_id(config: &Config) -> anyhow::Result<String> {
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    let git_commit = std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_string());
    let config_toml = toml::to_string(config)?;
    let mut hasher = Sha256::new();
    hasher.update(config_toml.as_bytes());
    let config_hash = hex::encode(hasher.finalize());
    Ok(format!("{}-{}-{}", now, git_commit, &config_hash[..8]))
}

fn mode_name(mode: InputMode) -> &'static str {
    match mode {
        InputMode::FixedInput => "fixed-input",
        InputMode::RandomInput => "random-input",
        InputMode::Chained => "chained",
    }
}

fn proof_name(algo: ProofAlgo) -> &'static str {
    match algo {
        ProofAlgo::Alg4 => "alg4",
        ProofAlgo::Alg5 => "alg5",
    }
}

#[derive(Debug, Serialize)]
struct SummaryReport {
    run_id: String,
    unit_type: String,
    target_duration_ns: Option<u128>,
    summary: crate::stats::StatsSummary,
}

fn jsonl_writer(path: &str) -> anyhow::Result<BufWriter<File>> {
    let file = OpenOptions::new().create(true).append(true).open(path)?;
    Ok(BufWriter::new(file))
}

fn write_jsonl<T: Serialize>(writer: &mut BufWriter<File>, value: &T) -> anyhow::Result<()> {
    let line = serde_json::to_string(value)?;
    writer.write_all(line.as_bytes())?;
    writer.write_all(b"\n")?;
    Ok(())
}

fn write_summary(path: &str, report: &SummaryReport) -> anyhow::Result<()> {
    let file = File::create(path)?;
    serde_json::to_writer_pretty(file, report)?;
    Ok(())
}

fn vdf_metadata(
    mode: InputMode,
    t: u64,
    k: usize,
    proof_algo: ProofAlgo,
    ok: bool,
    err_msg: &str,
) -> Value {
    serde_json::json!({
        "mode": mode_name(mode),
        "t": t,
        "k": k,
        "proof_algo": proof_name(proof_algo),
        "ok": ok,
        "err_msg": err_msg,
    })
}

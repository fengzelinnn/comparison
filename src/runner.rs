use crate::config::{Config, InputMode, ProofAlgo};
use crate::group::RsaGroup;
use crate::stats::summarize;
use crate::vdf::{eval, verify};
use csv::Writer;
use num_bigint::BigUint;
use rand::RngCore;
use rand::SeedableRng;
use rand_chacha::ChaCha20Rng;
use sha2::{Digest, Sha256};
use std::fs::OpenOptions;
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
        let mut durations = Vec::with_capacity(self.config.tasks.ticks);

        let mut writer = csv_writer(&self.config.storage.csv_path)?;
        writer.write_record([
            "run_id",
            "tick_index",
            "start_ns",
            "end_ns",
            "duration_ns",
            "mode",
            "t",
            "k",
            "proof_algo",
            "ok",
            "err_msg",
        ])?;

        for tick_index in 0..self.config.tasks.ticks {
            match self.config.tasks.mode {
                InputMode::FixedInput => {}
                InputMode::RandomInput => {
                    x = random_bytes(&mut rng, 32);
                }
                InputMode::Chained => {}
            }
            let start_wall = SystemTime::now();
            let start = Instant::now();
            let output = eval(&group, &x, self.config.vdf.t, self.config.vdf.k);
            let duration = start.elapsed();
            let end_wall = start_wall + duration;
            let ok = verify(
                &group,
                &output.g,
                &output.y,
                &output.proof,
                self.config.vdf.t,
                self.config.vdf.k,
            );
            let err_msg = if ok { "" } else { "verify_failed" };
            writer.write_record([
                run_id.as_str(),
                tick_index.to_string().as_str(),
                to_ns(start_wall)?.to_string().as_str(),
                to_ns(end_wall)?.to_string().as_str(),
                duration.as_nanos().to_string().as_str(),
                mode_name(self.config.tasks.mode),
                self.config.vdf.t.to_string().as_str(),
                self.config.vdf.k.to_string().as_str(),
                proof_name(self.config.vdf.proof_algo),
                ok.to_string().as_str(),
                err_msg,
            ])?;
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

        if let Some(summary) = summarize(&durations) {
            println!("run_id: {}", run_id);
            println!("mean_ns: {:.2}", summary.mean_ns);
            println!("std_ns: {:.2}", summary.std_ns);
            println!("p50_ns: {}", summary.p50_ns);
            println!("p90_ns: {}", summary.p90_ns);
            println!("p99_ns: {}", summary.p99_ns);
            println!("jitter_mean_ns: {:.2}", summary.jitter_mean_ns);
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

fn to_ns(time: SystemTime) -> anyhow::Result<u128> {
    Ok(time.duration_since(UNIX_EPOCH)?.as_nanos())
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

fn csv_writer(path: &str) -> anyhow::Result<Writer<std::fs::File>> {
    let file = OpenOptions::new().create(true).append(true).open(path)?;
    Ok(Writer::from_writer(file))
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

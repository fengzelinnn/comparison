#[derive(Debug, serde::Serialize)]
pub struct StatsSummary {
    pub mean_ns: f64,
    pub std_ns: f64,
    pub p50_ns: u128,
    pub p90_ns: u128,
    pub p99_ns: u128,
    pub jitter_mean_ns: f64,
    pub adj_jitter_mean_abs_ns: f64,
    pub adj_jitter_p99_abs_ns: u128,
    pub drift_max_pos_ns: i128,
    pub drift_max_neg_ns: i128,
    pub sample_count: usize,
    pub ticks_per_second_mean: f64,
}

pub fn summarize(durations: &[u128], target_duration_ns: Option<u128>) -> Option<StatsSummary> {
    if durations.is_empty() {
        return None;
    }
    let mean = durations.iter().copied().sum::<u128>() as f64 / durations.len() as f64;
    let variance = durations
        .iter()
        .map(|value| {
            let diff = *value as f64 - mean;
            diff * diff
        })
        .sum::<f64>()
        / durations.len() as f64;
    let std = variance.sqrt();
    let mut sorted = durations.to_vec();
    sorted.sort_unstable();
    let p50 = percentile(&sorted, 50.0);
    let p90 = percentile(&sorted, 90.0);
    let p99 = percentile(&sorted, 99.0);
    let jitter = durations
        .windows(2)
        .map(|pair| pair[1] as i128 - pair[0] as i128)
        .collect::<Vec<_>>();
    let jitter_mean = if jitter.is_empty() {
        0.0
    } else {
        jitter.iter().sum::<i128>() as f64 / jitter.len() as f64
    };
    let adj_jitter_abs: Vec<u128> = jitter.iter().map(|value| value.unsigned_abs()).collect();
    let adj_jitter_mean_abs = if adj_jitter_abs.is_empty() {
        0.0
    } else {
        adj_jitter_abs.iter().sum::<u128>() as f64 / adj_jitter_abs.len() as f64
    };
    let mut adj_sorted = adj_jitter_abs.clone();
    adj_sorted.sort_unstable();
    let adj_p99 = percentile(&adj_sorted, 99.0);
    let drift_base = target_duration_ns.map(|value| value as f64).unwrap_or(mean);
    let (drift_max_pos, drift_max_neg) = drift_extremes(durations, drift_base);
    let ticks_per_second_mean = if mean > 0.0 {
        1_000_000_000.0 / mean
    } else {
        0.0
    };
    Some(StatsSummary {
        mean_ns: mean,
        std_ns: std,
        p50_ns: p50,
        p90_ns: p90,
        p99_ns: p99,
        jitter_mean_ns: jitter_mean,
        adj_jitter_mean_abs_ns: adj_jitter_mean_abs,
        adj_jitter_p99_abs_ns: adj_p99,
        drift_max_pos_ns: drift_max_pos,
        drift_max_neg_ns: drift_max_neg,
        sample_count: durations.len(),
        ticks_per_second_mean,
    })
}

fn percentile(sorted: &[u128], pct: f64) -> u128 {
    if sorted.is_empty() {
        return 0;
    }
    let rank = ((pct / 100.0) * (sorted.len() as f64 - 1.0)).round() as usize;
    sorted[rank]
}

fn drift_extremes(durations: &[u128], mean: f64) -> (i128, i128) {
    let mut drift: i128 = 0;
    let mut max_pos: i128 = 0;
    let mut max_neg: i128 = 0;
    for value in durations {
        drift += *value as i128 - mean.round() as i128;
        if drift > max_pos {
            max_pos = drift;
        }
        if drift < max_neg {
            max_neg = drift;
        }
    }
    (max_pos, max_neg)
}

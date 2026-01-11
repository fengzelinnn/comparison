pub struct StatsSummary {
    pub mean_ns: f64,
    pub std_ns: f64,
    pub p50_ns: u128,
    pub p90_ns: u128,
    pub p99_ns: u128,
    pub jitter_mean_ns: f64,
}

pub fn summarize(durations: &[u128]) -> Option<StatsSummary> {
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
    Some(StatsSummary {
        mean_ns: mean,
        std_ns: std,
        p50_ns: p50,
        p90_ns: p90,
        p99_ns: p99,
        jitter_mean_ns: jitter_mean,
    })
}

fn percentile(sorted: &[u128], pct: f64) -> u128 {
    if sorted.is_empty() {
        return 0;
    }
    let rank = ((pct / 100.0) * (sorted.len() as f64 - 1.0)).round() as usize;
    sorted[rank]
}

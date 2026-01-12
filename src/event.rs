use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Serialize, Deserialize)]
pub struct TimeUnit {
    pub unit_id: u64,
    pub unit_type: String,
    pub target_duration_ns: Option<u128>,
    pub start_ts_ns: u128,
    pub end_ts_ns: u128,
    pub duration_ns: u128,
    pub work_amount: Option<u64>,
    pub proof_size_bytes: Option<usize>,
    pub verify_time_ns: Option<u128>,
    pub metadata: Value,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TimeUnitEvent {
    pub run_id: String,
    pub unit: TimeUnit,
}

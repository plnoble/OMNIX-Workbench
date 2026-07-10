//! Local-model selection IO layer (whichllm inspired): detect the machine's CPU
//! and RAM, then rank which open-weight models fit a memory budget. GPU VRAM
//! detection is unreliable cross-platform, so the user supplies a budget (their
//! VRAM for GPU inference, or RAM for CPU inference) — the panel defaults to RAM.

use serde::Serialize;
use sysinfo::System;

use crate::local_models::ModelRecommendation;

#[derive(Debug, Clone, Serialize)]
pub struct HardwareInfo {
    pub cpu_cores: usize,
    pub cpu_brand: String,
    pub ram_gb: f64,
}

/// Detect CPU cores/brand and total RAM (GB). Reliable cross-platform via sysinfo.
#[tauri::command]
pub fn detect_hardware() -> HardwareInfo {
    let sys = System::new_all();
    let ram_gb = sys.total_memory() as f64 / 1024.0 / 1024.0 / 1024.0;
    let cpus = sys.cpus();
    HardwareInfo {
        cpu_cores: cpus.len(),
        cpu_brand: cpus.first().map(|c| c.brand().trim().to_string()).unwrap_or_default(),
        ram_gb: (ram_gb * 10.0).round() / 10.0,
    }
}

/// Rank local models that fit a memory budget (GB). Pure ranking in
/// `crate::local_models` — see its unit tests.
#[tauri::command]
pub fn recommend_local_models(budget_gb: f64) -> Vec<ModelRecommendation> {
    crate::local_models::rank_models(budget_gb.max(0.5))
}

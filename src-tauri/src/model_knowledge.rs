//! Model Knowledge Base
//!
//! Provides hardware-aware model recommendations with:
//! - Evidence-graded confidence scoring (5-tier system)
//! - Lineage-aware version management (model family generations)
//! - GPU simulation for hardware planning
//! - Curated model database with quality ratings

use crate::proc::NoWindow;
use serde::{Deserialize, Serialize};

// ══════════════════════════════════════════════════
// Evidence Confidence System
// ══════════════════════════════════════════════════

/// Evidence tier for model quality rating confidence
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum EvidenceTier {
    /// Installed locally, verified working — full confidence
    Direct,
    /// Same family different quant — high confidence
    Variant,
    /// Inherited from base model — medium confidence
    BaseModel,
    /// Interpolated within family by size — low confidence
    LineInterp,
    /// Community/uploader claimed — lowest confidence
    SelfReported,
}

impl EvidenceTier {
    pub fn confidence(&self) -> f32 {
        match self {
            Self::Direct => 1.0,
            Self::Variant => 0.7,
            Self::BaseModel => 0.6,
            Self::LineInterp => 0.3,
            Self::SelfReported => 0.4,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Direct => "已安装",
            Self::Variant => "同系列",
            Self::BaseModel => "基础模型",
            Self::LineInterp => "估算",
            Self::SelfReported => "社区",
        }
    }

    pub fn icon(&self) -> &'static str {
        match self {
            Self::Direct => "🟢",
            Self::Variant => "🔵",
            Self::BaseModel => "🟡",
            Self::LineInterp => "🟠",
            Self::SelfReported => "🔴",
        }
    }
}

// ══════════════════════════════════════════════════
// Model Lineage System
// ══════════════════════════════════════════════════

/// Model family and generation info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelLineage {
    pub family: String,
    pub generation: u32,
}

/// Detect model family and generation from name
pub fn detect_lineage(name: &str) -> ModelLineage {
    let lower = name.to_lowercase();

    // Qwen family
    if lower.contains("qwen3") { return ModelLineage { family: "Qwen".into(), generation: 3 }; }
    if lower.contains("qwen2.5") { return ModelLineage { family: "Qwen".into(), generation: 2 }; }
    if lower.contains("qwen2") { return ModelLineage { family: "Qwen".into(), generation: 2 }; }
    if lower.contains("qwen") { return ModelLineage { family: "Qwen".into(), generation: 1 }; }

    // Llama family
    if lower.contains("llama-4") || lower.contains("llama4") { return ModelLineage { family: "Llama".into(), generation: 4 }; }
    if lower.contains("llama-3.3") || lower.contains("llama3.3") { return ModelLineage { family: "Llama".into(), generation: 3 }; }
    if lower.contains("llama-3.2") || lower.contains("llama3.2") { return ModelLineage { family: "Llama".into(), generation: 3 }; }
    if lower.contains("llama-3.1") || lower.contains("llama3.1") { return ModelLineage { family: "Llama".into(), generation: 3 }; }
    if lower.contains("llama-3") || lower.contains("llama3") { return ModelLineage { family: "Llama".into(), generation: 3 }; }
    if lower.contains("llama-2") || lower.contains("llama2") { return ModelLineage { family: "Llama".into(), generation: 2 }; }

    // DeepSeek family
    if lower.contains("deepseek-v4") || lower.contains("deepseek-r2") { return ModelLineage { family: "DeepSeek".into(), generation: 4 }; }
    if lower.contains("deepseek-v3") || lower.contains("deepseek-r1") { return ModelLineage { family: "DeepSeek".into(), generation: 3 }; }
    if lower.contains("deepseek-v2") { return ModelLineage { family: "DeepSeek".into(), generation: 2 }; }

    // Gemma family
    if lower.contains("gemma-3") || lower.contains("gemma3") { return ModelLineage { family: "Gemma".into(), generation: 3 }; }
    if lower.contains("gemma-2") || lower.contains("gemma2") { return ModelLineage { family: "Gemma".into(), generation: 2 }; }

    // Phi family
    if lower.contains("phi-4") { return ModelLineage { family: "Phi".into(), generation: 4 }; }
    if lower.contains("phi-3") { return ModelLineage { family: "Phi".into(), generation: 3 }; }

    // Mistral family
    if lower.contains("mistral-large") || lower.contains("mistral-nemo") { return ModelLineage { family: "Mistral".into(), generation: 3 }; }
    if lower.contains("mistral") { return ModelLineage { family: "Mistral".into(), generation: 2 }; }

    // GLM family
    if lower.contains("glm-4") { return ModelLineage { family: "GLM".into(), generation: 4 }; }

    ModelLineage { family: "Unknown".into(), generation: 1 }
}

/// Calculate generation penalty (older generations get demoted)
pub fn generation_penalty(family: &str, generation: u32) -> f32 {
    // Find the latest generation for this family
    let latest = match family {
        "Qwen" => 3,
        "Llama" => 4,
        "DeepSeek" => 4,
        "Gemma" => 3,
        "Phi" => 4,
        "Mistral" => 3,
        "GLM" => 4,
        _ => return 1.0,
    };

    let age = (latest as i32 - generation as i32).max(0) as u32;
    if age == 0 { return 1.0; }
    // Demote by 12% per generation, floor at 0.55
    (1.0 - 0.12 * age as f32).max(0.55)
}

// ══════════════════════════════════════════════════
// Model Entry with Evidence Grading
// ══════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelEntry {
    pub name: String,
    pub display_name: String,
    pub size_gb: f32,
    pub min_vram_gb: f32,
    pub categories: Vec<String>,
    pub quality: u32,
    pub description: String,
    pub ollama_cmd: String,
    pub speed_rating: String,
    // evidence / lineage fields
    pub family: String,
    pub generation: u32,
    pub evidence_tier: EvidenceTier,
    pub confidence: f32,
    pub is_moe: bool,
    pub active_params_gb: Option<f32>,
}

// ══════════════════════════════════════════════════
// GPU Simulation
// ══════════════════════════════════════════════════

/// Curated GPU registry with bandwidth and VRAM
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuSpec {
    pub name: String,
    pub vram_mb: u32,
    pub bandwidth_gb_s: f32,
    pub vendor: String,
    pub generation: String,
}

/// GPU database — curated specs.
pub fn get_gpu_database() -> Vec<GpuSpec> {
    vec![
        // NVIDIA RTX 50 series
        GpuSpec { name: "RTX 5090".into(), vram_mb: 32768, bandwidth_gb_s: 1792.0, vendor: "NVIDIA".into(), generation: "Blackwell".into() },
        GpuSpec { name: "RTX 5080".into(), vram_mb: 16384, bandwidth_gb_s: 960.0, vendor: "NVIDIA".into(), generation: "Blackwell".into() },
        GpuSpec { name: "RTX 5070 Ti".into(), vram_mb: 16384, bandwidth_gb_s: 896.0, vendor: "NVIDIA".into(), generation: "Blackwell".into() },
        GpuSpec { name: "RTX 5070".into(), vram_mb: 12288, bandwidth_gb_s: 672.0, vendor: "NVIDIA".into(), generation: "Blackwell".into() },
        // NVIDIA RTX 40 series
        GpuSpec { name: "RTX 4090".into(), vram_mb: 24576, bandwidth_gb_s: 1008.0, vendor: "NVIDIA".into(), generation: "Ada Lovelace".into() },
        GpuSpec { name: "RTX 4080 Super".into(), vram_mb: 16384, bandwidth_gb_s: 736.0, vendor: "NVIDIA".into(), generation: "Ada Lovelace".into() },
        GpuSpec { name: "RTX 4070 Ti Super".into(), vram_mb: 16384, bandwidth_gb_s: 672.0, vendor: "NVIDIA".into(), generation: "Ada Lovelace".into() },
        GpuSpec { name: "RTX 4070 Ti".into(), vram_mb: 12288, bandwidth_gb_s: 504.0, vendor: "NVIDIA".into(), generation: "Ada Lovelace".into() },
        GpuSpec { name: "RTX 4070 Super".into(), vram_mb: 12288, bandwidth_gb_s: 504.0, vendor: "NVIDIA".into(), generation: "Ada Lovelace".into() },
        GpuSpec { name: "RTX 4070".into(), vram_mb: 12288, bandwidth_gb_s: 504.0, vendor: "NVIDIA".into(), generation: "Ada Lovelace".into() },
        GpuSpec { name: "RTX 4060 Ti 16GB".into(), vram_mb: 16384, bandwidth_gb_s: 288.0, vendor: "NVIDIA".into(), generation: "Ada Lovelace".into() },
        GpuSpec { name: "RTX 4060 Ti".into(), vram_mb: 8192, bandwidth_gb_s: 288.0, vendor: "NVIDIA".into(), generation: "Ada Lovelace".into() },
        GpuSpec { name: "RTX 4060".into(), vram_mb: 8192, bandwidth_gb_s: 272.0, vendor: "NVIDIA".into(), generation: "Ada Lovelace".into() },
        // NVIDIA RTX 30 series
        GpuSpec { name: "RTX 3090 Ti".into(), vram_mb: 24576, bandwidth_gb_s: 1008.0, vendor: "NVIDIA".into(), generation: "Ampere".into() },
        GpuSpec { name: "RTX 3090".into(), vram_mb: 24576, bandwidth_gb_s: 936.0, vendor: "NVIDIA".into(), generation: "Ampere".into() },
        GpuSpec { name: "RTX 3080 Ti".into(), vram_mb: 12288, bandwidth_gb_s: 912.0, vendor: "NVIDIA".into(), generation: "Ampere".into() },
        GpuSpec { name: "RTX 3080".into(), vram_mb: 10240, bandwidth_gb_s: 760.0, vendor: "NVIDIA".into(), generation: "Ampere".into() },
        GpuSpec { name: "RTX 3070 Ti".into(), vram_mb: 8192, bandwidth_gb_s: 672.0, vendor: "NVIDIA".into(), generation: "Ampere".into() },
        GpuSpec { name: "RTX 3070".into(), vram_mb: 8192, bandwidth_gb_s: 448.0, vendor: "NVIDIA".into(), generation: "Ampere".into() },
        GpuSpec { name: "RTX 3060".into(), vram_mb: 12288, bandwidth_gb_s: 360.0, vendor: "NVIDIA".into(), generation: "Ampere".into() },
        // NVIDIA Datacenter
        GpuSpec { name: "H200".into(), vram_mb: 131072, bandwidth_gb_s: 4800.0, vendor: "NVIDIA".into(), generation: "Hopper".into() },
        GpuSpec { name: "H100".into(), vram_mb: 81920, bandwidth_gb_s: 3350.0, vendor: "NVIDIA".into(), generation: "Hopper".into() },
        GpuSpec { name: "A100 80GB".into(), vram_mb: 81920, bandwidth_gb_s: 2039.0, vendor: "NVIDIA".into(), generation: "Ampere".into() },
        GpuSpec { name: "A100 40GB".into(), vram_mb: 40960, bandwidth_gb_s: 1555.0, vendor: "NVIDIA".into(), generation: "Ampere".into() },
        // AMD
        GpuSpec { name: "RX 7900 XTX".into(), vram_mb: 24576, bandwidth_gb_s: 960.0, vendor: "AMD".into(), generation: "RDNA 3".into() },
        GpuSpec { name: "RX 7900 XT".into(), vram_mb: 20480, bandwidth_gb_s: 800.0, vendor: "AMD".into(), generation: "RDNA 3".into() },
        GpuSpec { name: "RX 7800 XT".into(), vram_mb: 16384, bandwidth_gb_s: 624.0, vendor: "AMD".into(), generation: "RDNA 3".into() },
        GpuSpec { name: "RX 7600".into(), vram_mb: 8192, bandwidth_gb_s: 288.0, vendor: "AMD".into(), generation: "RDNA 3".into() },
        GpuSpec { name: "RX 9070 XT".into(), vram_mb: 16384, bandwidth_gb_s: 640.0, vendor: "AMD".into(), generation: "RDNA 4".into() },
        GpuSpec { name: "RX 9070".into(), vram_mb: 16384, bandwidth_gb_s: 576.0, vendor: "AMD".into(), generation: "RDNA 4".into() },
        // Apple Silicon
        GpuSpec { name: "M4 Max".into(), vram_mb: 131072, bandwidth_gb_s: 546.0, vendor: "Apple".into(), generation: "M4".into() },
        GpuSpec { name: "M4 Pro".into(), vram_mb: 65536, bandwidth_gb_s: 273.0, vendor: "Apple".into(), generation: "M4".into() },
        GpuSpec { name: "M3 Ultra".into(), vram_mb: 196608, bandwidth_gb_s: 800.0, vendor: "Apple".into(), generation: "M3".into() },
        GpuSpec { name: "M3 Max".into(), vram_mb: 131072, bandwidth_gb_s: 546.0, vendor: "Apple".into(), generation: "M3".into() },
        GpuSpec { name: "M2 Ultra".into(), vram_mb: 196608, bandwidth_gb_s: 800.0, vendor: "Apple".into(), generation: "M2".into() },
        GpuSpec { name: "M2 Max".into(), vram_mb: 98304, bandwidth_gb_s: 400.0, vendor: "Apple".into(), generation: "M2".into() },
        GpuSpec { name: "M1 Max".into(), vram_mb: 65536, bandwidth_gb_s: 400.0, vendor: "Apple".into(), generation: "M1".into() },
        GpuSpec { name: "M1".into(), vram_mb: 16384, bandwidth_gb_s: 68.0, vendor: "Apple".into(), generation: "M1".into() },
    ]
}

/// Simulate a GPU by name string
pub fn simulate_gpu(name: &str) -> Option<GpuSpec> {
    let lower = name.to_lowercase();
    get_gpu_database().into_iter().find(|g| {
        g.name.to_lowercase().contains(&lower) || lower.contains(&g.name.to_lowercase())
    })
}

// ══════════════════════════════════════════════════
// Hardware Detection
// ══════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuInfo {
    pub name: String,
    pub vram_mb: u32,
    pub vendor: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HardwareInfo {
    pub gpu: Option<GpuInfo>,
    pub ram_mb: u32,
    pub cpu_cores: u32,
    pub os: String,
}

pub fn detect_hardware() -> HardwareInfo {
    let os = if cfg!(windows) { "Windows" } else if cfg!(target_os = "macos") { "macOS" } else { "Linux" };
    let gpu = detect_gpu();
    let ram_mb = detect_ram_mb();
    let cpu_cores = std::thread::available_parallelism().map(|n| n.get() as u32).unwrap_or(4);
    HardwareInfo { gpu, ram_mb, cpu_cores, os: os.to_string() }
}

fn detect_gpu() -> Option<GpuInfo> {
    let output = std::process::Command::new("nvidia-smi")
        .args(["--query-gpu=name,memory.total", "--format=csv,noheader,nounits"])
        .no_window()
        .output().ok()?;
    if !output.status.success() { return None; }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let line = stdout.lines().next()?;
    let parts: Vec<&str> = line.split(',').collect();
    if parts.len() < 2 { return None; }
    Some(GpuInfo {
        name: parts[0].trim().to_string(),
        vram_mb: parts[1].trim().parse().ok()?,
        vendor: "NVIDIA".into(),
    })
}

fn detect_ram_mb() -> u32 {
    #[cfg(windows)]
    {
        std::process::Command::new("powershell")
            .args(["-Command", "(Get-CimInstance Win32_OperatingSystem).TotalVisibleMemorySize"])
            .no_window()
            .output().ok()
            .and_then(|o| String::from_utf8_lossy(&o.stdout).trim().parse::<u32>().ok())
            .map(|kb| kb / 1024)
            .unwrap_or(16384)
    }
    #[cfg(not(windows))]
    {
        std::fs::read_to_string("/proc/meminfo").ok()
            .and_then(|s| s.lines().find(|l| l.starts_with("MemTotal:"))
                .and_then(|l| l.split_whitespace().nth(1))
                .and_then(|v| v.parse::<u32>().ok()))
            .map(|kb| kb / 1024)
            .unwrap_or(16384)
    }
}

// ══════════════════════════════════════════════════
// Model Database with Evidence Grading
// ══════════════════════════════════════════════════

pub fn get_model_database() -> Vec<ModelEntry> {
    vec![
        // ── Small models (2-5GB) ──
        ModelEntry {
            name: "qwen2.5:3b".into(), display_name: "Qwen 2.5 3B".into(),
            size_gb: 2.0, min_vram_gb: 4.0,
            categories: vec!["通用".into(), "中文".into()],
            quality: 6, description: "通义千问轻量版，中文对话能力不错".into(),
            ollama_cmd: "ollama pull qwen2.5:3b".into(), speed_rating: "fast".into(),
            family: "Qwen".into(), generation: 2,
            evidence_tier: EvidenceTier::BaseModel, confidence: 0.6,
            is_moe: false, active_params_gb: None,
        },
        ModelEntry {
            name: "phi-4:4b".into(), display_name: "Phi-4 4B".into(),
            size_gb: 2.5, min_vram_gb: 4.0,
            categories: vec!["推理".into(), "代码".into()],
            quality: 7, description: "微软 Phi-4，推理能力强".into(),
            ollama_cmd: "ollama pull phi-4:4b".into(), speed_rating: "fast".into(),
            family: "Phi".into(), generation: 4,
            evidence_tier: EvidenceTier::BaseModel, confidence: 0.6,
            is_moe: false, active_params_gb: None,
        },
        ModelEntry {
            name: "gemma3:4b".into(), display_name: "Gemma 3 4B".into(),
            size_gb: 3.0, min_vram_gb: 5.0,
            categories: vec!["通用".into(), "多模态".into()],
            quality: 7, description: "Google Gemma 3，支持图片理解".into(),
            ollama_cmd: "ollama pull gemma3:4b".into(), speed_rating: "fast".into(),
            family: "Gemma".into(), generation: 3,
            evidence_tier: EvidenceTier::BaseModel, confidence: 0.6,
            is_moe: false, active_params_gb: None,
        },
        // ── Medium models (5-10GB) ──
        ModelEntry {
            name: "qwen2.5:7b".into(), display_name: "Qwen 2.5 7B".into(),
            size_gb: 4.7, min_vram_gb: 6.0,
            categories: vec!["通用".into(), "中文".into(), "代码".into()],
            quality: 8, description: "通义千问 7B，性价比最高".into(),
            ollama_cmd: "ollama pull qwen2.5:7b".into(), speed_rating: "medium".into(),
            family: "Qwen".into(), generation: 2,
            evidence_tier: EvidenceTier::BaseModel, confidence: 0.6,
            is_moe: false, active_params_gb: None,
        },
        ModelEntry {
            name: "deepseek-r1:8b".into(), display_name: "DeepSeek R1 8B".into(),
            size_gb: 4.9, min_vram_gb: 6.0,
            categories: vec!["推理".into(), "代码".into()],
            quality: 8, description: "DeepSeek R1 蒸馏版，思维链推理".into(),
            ollama_cmd: "ollama pull deepseek-r1:8b".into(), speed_rating: "medium".into(),
            family: "DeepSeek".into(), generation: 3,
            evidence_tier: EvidenceTier::Variant, confidence: 0.7,
            is_moe: false, active_params_gb: None,
        },
        ModelEntry {
            name: "llama3.1:8b".into(), display_name: "Llama 3.1 8B".into(),
            size_gb: 4.7, min_vram_gb: 6.0,
            categories: vec!["通用".into(), "英文".into()],
            quality: 7, description: "Meta Llama 3.1 8B，英文通用".into(),
            ollama_cmd: "ollama pull llama3.1:8b".into(), speed_rating: "medium".into(),
            family: "Llama".into(), generation: 3,
            evidence_tier: EvidenceTier::BaseModel, confidence: 0.6,
            is_moe: false, active_params_gb: None,
        },
        // ── Large models (10-20GB) ──
        ModelEntry {
            name: "qwen2.5:14b".into(), display_name: "Qwen 2.5 14B".into(),
            size_gb: 8.9, min_vram_gb: 12.0,
            categories: vec!["通用".into(), "中文".into(), "代码".into()],
            quality: 9, description: "通义千问 14B，接近 GPT-4 水平".into(),
            ollama_cmd: "ollama pull qwen2.5:14b".into(), speed_rating: "slow".into(),
            family: "Qwen".into(), generation: 2,
            evidence_tier: EvidenceTier::BaseModel, confidence: 0.6,
            is_moe: false, active_params_gb: None,
        },
        ModelEntry {
            name: "deepseek-coder-v2:16b".into(), display_name: "DeepSeek Coder V2 16B".into(),
            size_gb: 9.0, min_vram_gb: 12.0,
            categories: vec!["代码".into()],
            quality: 9, description: "DeepSeek 代码专精模型".into(),
            ollama_cmd: "ollama pull deepseek-coder-v2:16b".into(), speed_rating: "slow".into(),
            family: "DeepSeek".into(), generation: 2,
            evidence_tier: EvidenceTier::Variant, confidence: 0.7,
            is_moe: true, active_params_gb: Some(2.4),
        },
        // ── Very large models (20GB+) ──
        ModelEntry {
            name: "qwen2.5:32b".into(), display_name: "Qwen 2.5 32B".into(),
            size_gb: 20.0, min_vram_gb: 24.0,
            categories: vec!["通用".into(), "中文".into(), "代码".into()],
            quality: 9, description: "通义千问 32B，高端消费级显卡可用".into(),
            ollama_cmd: "ollama pull qwen2.5:32b".into(), speed_rating: "slow".into(),
            family: "Qwen".into(), generation: 2,
            evidence_tier: EvidenceTier::BaseModel, confidence: 0.6,
            is_moe: false, active_params_gb: None,
        },
    ]
}

// ══════════════════════════════════════════════════
// Recommendation Engine
// ══════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelRecommendation {
    pub model: ModelEntry,
    pub fits_vram: bool,
    pub fits_ram: bool,
    pub overall_fit: String,
    pub install_cmd: String,
    pub effective_quality: f32,
    pub confidence_label: String,
}

pub fn recommend_models(hw: &HardwareInfo) -> Vec<ModelRecommendation> {
    let db = get_model_database();
    let vram = hw.gpu.as_ref().map(|g| g.vram_mb as f32 / 1024.0).unwrap_or(0.0);
    let ram = hw.ram_mb as f32 / 1024.0;

    let mut recommendations: Vec<ModelRecommendation> = db.into_iter().map(|model| {
        let fits_vram = vram >= model.min_vram_gb;
        let fits_ram = ram >= model.size_gb * 1.5;

        let overall_fit = if fits_vram { "perfect".to_string() }
            else if fits_ram { "tight".to_string() }
            else { "impossible".to_string() };

        // Apply lineage penalty
        let gen_penalty = generation_penalty(&model.family, model.generation);
        let evidence_conf = model.confidence;
        let effective_quality = model.quality as f32 * gen_penalty * evidence_conf;

        ModelRecommendation {
            install_cmd: model.ollama_cmd.clone(),
            confidence_label: format!("{} {}", model.evidence_tier.icon(), model.evidence_tier.label()),
            model,
            fits_vram,
            fits_ram,
            overall_fit,
            effective_quality,
        }
    }).collect();

    // Sort by effective quality descending
    recommendations.sort_by(|a, b| {
        let order = |s: &str| match s { "perfect" => 0, "tight" => 1, _ => 2 };
        order(&a.overall_fit).cmp(&order(&b.overall_fit))
            .then(b.effective_quality.partial_cmp(&a.effective_quality).unwrap_or(std::cmp::Ordering::Equal))
    });

    recommendations
}

/// Simulate recommendations for a hypothetical GPU
pub fn recommend_for_gpu(gpu_name: &str) -> Result<Vec<ModelRecommendation>, String> {
    let gpu = simulate_gpu(gpu_name).ok_or_else(|| format!("Unknown GPU: {}", gpu_name))?;
    let hw = HardwareInfo {
        gpu: Some(GpuInfo { name: gpu.name, vram_mb: gpu.vram_mb, vendor: gpu.vendor }),
        ram_mb: 32768, // Assume 32GB RAM for simulation
        cpu_cores: 8,
        os: "Simulation".into(),
    };
    Ok(recommend_models(&hw))
}

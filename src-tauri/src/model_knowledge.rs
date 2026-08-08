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

/// 远程目录的默认地址。放 GitHub raw，改模型不用发版。
const REMOTE_CATALOG_URL: &str =
    "https://raw.githubusercontent.com/plnoble/OMNIX-Workbench/master/resources/model-catalog.json";

/// 缓存下来的远程目录（进程内）。`None` = 还没拉过或拉失败。
static REMOTE_CATALOG: std::sync::OnceLock<std::sync::RwLock<Option<Vec<ModelEntry>>>> =
    std::sync::OnceLock::new();

fn remote_catalog() -> &'static std::sync::RwLock<Option<Vec<ModelEntry>>> {
    REMOTE_CATALOG.get_or_init(|| std::sync::RwLock::new(None))
}

/// 拉一次远程目录。失败**不报错**——内置副本永远兜底，离线照样能用。
///
/// 目录以前是硬编码的，加一个模型要发一次版。搬到可远程更新的 JSON 之后，
/// 维护那个文件就够了；而内置那份仍然完整保留，网络不通时表现和以前一模一样。
pub async fn refresh_remote_catalog(url: Option<&str>) -> Result<usize, String> {
    let url = url.unwrap_or(REMOTE_CATALOG_URL);
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| e.to_string())?;
    let response = client.get(url).send().await.map_err(|e| {
        format!("拉取模型目录失败：{}", crate::proxy::describe_request_error(&e))
    })?;
    if !response.status().is_success() {
        return Err(format!("模型目录返回 {}", response.status()));
    }
    let entries: Vec<ModelEntry> = response
        .json()
        .await
        .map_err(|e| format!("模型目录格式不对：{e}"))?;
    if entries.is_empty() {
        return Err("远程目录是空的，保留内置副本".into());
    }
    let count = entries.len();
    if let Ok(mut slot) = remote_catalog().write() {
        *slot = Some(entries);
    }
    Ok(count)
}

/// 当前生效的模型目录：远程拉到了就用远程的，否则用内置副本。
pub fn get_model_database() -> Vec<ModelEntry> {
    if let Ok(slot) = remote_catalog().read() {
        if let Some(entries) = slot.as_ref() {
            return entries.clone();
        }
    }
    builtin_model_database()
}

/// 内置副本。远程拉不到时的兜底，也是远程 JSON 的格式样板。
///
/// 每一条的 tag 和体积都是**对着 Ollama registry 核过的**（
/// `registry.ollama.ai/v2/library/<名>/manifests/<档>`，体积取 model 层的字节数），
/// 不是照着印象写的。上一版里的 `phi-4:4b` 就不存在——registry 返回 404，点下载
/// 只会失败。
fn builtin_model_database() -> Vec<ModelEntry> {
    vec![
        // ── 极小：核显 / 老笔记本 ──
        ModelEntry {
            name: "gemma3:1b".into(), display_name: "Gemma 3 1B".into(),
            size_gb: 0.8, min_vram_gb: 2.0,
            categories: vec!["通用".into()],
            quality: 4, description: "最小的可用模型，核显和老笔记本也能跑".into(),
            ollama_cmd: "ollama pull gemma3:1b".into(), speed_rating: "fast".into(),
            family: "Gemma".into(), generation: 3,
            evidence_tier: EvidenceTier::BaseModel, confidence: 0.6,
            is_moe: false, active_params_gb: None,
        },
        ModelEntry {
            name: "qwen3:1.7b".into(), display_name: "Qwen3 1.7B".into(),
            size_gb: 1.4, min_vram_gb: 3.0,
            categories: vec!["通用".into(), "中文".into()],
            quality: 5, description: "千问最小档，中文尚可，适合做本地小工具".into(),
            ollama_cmd: "ollama pull qwen3:1.7b".into(), speed_rating: "fast".into(),
            family: "Qwen".into(), generation: 3,
            evidence_tier: EvidenceTier::BaseModel, confidence: 0.6,
            is_moe: false, active_params_gb: None,
        },
        // ── 小：4GB 显存 ──
        ModelEntry {
            name: "llama3.2:3b".into(), display_name: "Llama 3.2 3B".into(),
            size_gb: 2.0, min_vram_gb: 4.0,
            categories: vec!["通用".into(), "英文".into()],
            quality: 5, description: "Meta 小模型，英文对话流畅".into(),
            ollama_cmd: "ollama pull llama3.2:3b".into(), speed_rating: "fast".into(),
            family: "Llama".into(), generation: 3,
            evidence_tier: EvidenceTier::BaseModel, confidence: 0.6,
            is_moe: false, active_params_gb: None,
        },
        ModelEntry {
            name: "qwen3:4b".into(), display_name: "Qwen3 4B".into(),
            size_gb: 2.5, min_vram_gb: 4.0,
            categories: vec!["通用".into(), "中文".into(), "推理".into()],
            quality: 7, description: "小体积里最均衡的一档，带思考模式".into(),
            ollama_cmd: "ollama pull qwen3:4b".into(), speed_rating: "fast".into(),
            family: "Qwen".into(), generation: 3,
            evidence_tier: EvidenceTier::BaseModel, confidence: 0.6,
            is_moe: false, active_params_gb: None,
        },
        ModelEntry {
            name: "phi4-mini".into(), display_name: "Phi-4 Mini".into(),
            size_gb: 2.5, min_vram_gb: 4.0,
            categories: vec!["推理".into(), "代码".into()],
            quality: 6, description: "微软小钢炮，数学和推理超出体积预期".into(),
            ollama_cmd: "ollama pull phi4-mini".into(), speed_rating: "fast".into(),
            family: "Phi".into(), generation: 4,
            evidence_tier: EvidenceTier::BaseModel, confidence: 0.6,
            is_moe: false, active_params_gb: None,
        },
        ModelEntry {
            name: "gemma3:4b".into(), display_name: "Gemma 3 4B".into(),
            size_gb: 3.3, min_vram_gb: 5.0,
            categories: vec!["通用".into(), "多模态".into()],
            quality: 7, description: "Google Gemma 3，能看图，支持 140+ 语言".into(),
            ollama_cmd: "ollama pull gemma3:4b".into(), speed_rating: "fast".into(),
            family: "Gemma".into(), generation: 3,
            evidence_tier: EvidenceTier::BaseModel, confidence: 0.6,
            is_moe: false, active_params_gb: None,
        },
        // ── 中：6-8GB 显存 ──
        ModelEntry {
            name: "qwen2.5-coder:7b".into(), display_name: "Qwen2.5 Coder 7B".into(),
            size_gb: 4.7, min_vram_gb: 6.0,
            categories: vec!["代码".into()],
            quality: 7, description: "小显存里最好用的代码补全模型".into(),
            ollama_cmd: "ollama pull qwen2.5-coder:7b".into(), speed_rating: "medium".into(),
            family: "Qwen".into(), generation: 2,
            evidence_tier: EvidenceTier::Variant, confidence: 0.7,
            is_moe: false, active_params_gb: None,
        },
        ModelEntry {
            name: "llama3.1:8b".into(), display_name: "Llama 3.1 8B".into(),
            size_gb: 4.7, min_vram_gb: 6.0,
            categories: vec!["通用".into(), "英文".into()],
            quality: 7, description: "Meta Llama 3.1，英文通用老牌选择".into(),
            ollama_cmd: "ollama pull llama3.1:8b".into(), speed_rating: "medium".into(),
            family: "Llama".into(), generation: 3,
            evidence_tier: EvidenceTier::BaseModel, confidence: 0.6,
            is_moe: false, active_params_gb: None,
        },
        ModelEntry {
            name: "qwen3:8b".into(), display_name: "Qwen3 8B".into(),
            size_gb: 5.2, min_vram_gb: 7.0,
            categories: vec!["通用".into(), "中文".into(), "推理".into()],
            quality: 8, description: "8GB 显卡上的默认答案，中英都强".into(),
            ollama_cmd: "ollama pull qwen3:8b".into(), speed_rating: "medium".into(),
            family: "Qwen".into(), generation: 3,
            evidence_tier: EvidenceTier::BaseModel, confidence: 0.6,
            is_moe: false, active_params_gb: None,
        },
        ModelEntry {
            name: "deepseek-r1:8b".into(), display_name: "DeepSeek R1 8B".into(),
            size_gb: 5.2, min_vram_gb: 7.0,
            categories: vec!["推理".into(), "代码".into()],
            quality: 8, description: "DeepSeek R1 蒸馏版，会写出思维链".into(),
            ollama_cmd: "ollama pull deepseek-r1:8b".into(), speed_rating: "medium".into(),
            family: "DeepSeek".into(), generation: 3,
            evidence_tier: EvidenceTier::Variant, confidence: 0.7,
            is_moe: false, active_params_gb: None,
        },
        // ── 中大：10-12GB 显存 ──
        ModelEntry {
            name: "gemma3:12b".into(), display_name: "Gemma 3 12B".into(),
            size_gb: 8.1, min_vram_gb: 10.0,
            categories: vec!["通用".into(), "多模态".into()],
            quality: 8, description: "能看图的中量级，12GB 显卡刚好".into(),
            ollama_cmd: "ollama pull gemma3:12b".into(), speed_rating: "medium".into(),
            family: "Gemma".into(), generation: 3,
            evidence_tier: EvidenceTier::BaseModel, confidence: 0.6,
            is_moe: false, active_params_gb: None,
        },
        ModelEntry {
            name: "deepseek-r1:14b".into(), display_name: "DeepSeek R1 14B".into(),
            size_gb: 9.0, min_vram_gb: 11.0,
            categories: vec!["推理".into(), "代码".into()],
            quality: 8, description: "R1 蒸馏 14B，推理质量明显高过 8B".into(),
            ollama_cmd: "ollama pull deepseek-r1:14b".into(), speed_rating: "slow".into(),
            family: "DeepSeek".into(), generation: 3,
            evidence_tier: EvidenceTier::Variant, confidence: 0.7,
            is_moe: false, active_params_gb: None,
        },
        ModelEntry {
            name: "qwen3:14b".into(), display_name: "Qwen3 14B".into(),
            size_gb: 9.3, min_vram_gb: 12.0,
            categories: vec!["通用".into(), "中文".into(), "代码".into()],
            quality: 9, description: "12GB 卡的上限，综合能力接近云端中端模型".into(),
            ollama_cmd: "ollama pull qwen3:14b".into(), speed_rating: "slow".into(),
            family: "Qwen".into(), generation: 3,
            evidence_tier: EvidenceTier::BaseModel, confidence: 0.6,
            is_moe: false, active_params_gb: None,
        },
        // ── 大：16-24GB 显存 ──
        ModelEntry {
            name: "gpt-oss:20b".into(), display_name: "GPT-OSS 20B".into(),
            size_gb: 13.8, min_vram_gb: 16.0,
            categories: vec!["通用".into(), "推理".into(), "代码".into()],
            quality: 9, description: "OpenAI 开放权重，MoE 每次只激活 3.6B，16GB 内存就能跑得动".into(),
            ollama_cmd: "ollama pull gpt-oss:20b".into(), speed_rating: "medium".into(),
            family: "GPT-OSS".into(), generation: 1,
            evidence_tier: EvidenceTier::BaseModel, confidence: 0.6,
            is_moe: true, active_params_gb: Some(3.6),
        },
        ModelEntry {
            name: "devstral:24b".into(), display_name: "Devstral 24B".into(),
            size_gb: 14.3, min_vram_gb: 17.0,
            categories: vec!["代码".into(), "智能体".into()],
            quality: 9, description: "专为 agent 工作流训练的编码模型，SWE-Bench 成绩是本地模型里最硬的".into(),
            ollama_cmd: "ollama pull devstral:24b".into(), speed_rating: "slow".into(),
            family: "Mistral".into(), generation: 1,
            evidence_tier: EvidenceTier::Variant, confidence: 0.7,
            is_moe: false, active_params_gb: None,
        },
        ModelEntry {
            name: "mistral-small3.2:24b".into(), display_name: "Mistral Small 3.2 24B".into(),
            size_gb: 15.2, min_vram_gb: 18.0,
            categories: vec!["通用".into(), "多模态".into()],
            quality: 8, description: "Mistral 中量级，支持图片和函数调用".into(),
            ollama_cmd: "ollama pull mistral-small3.2:24b".into(), speed_rating: "slow".into(),
            family: "Mistral".into(), generation: 3,
            evidence_tier: EvidenceTier::BaseModel, confidence: 0.6,
            is_moe: false, active_params_gb: None,
        },
        ModelEntry {
            name: "gemma3:27b".into(), display_name: "Gemma 3 27B".into(),
            size_gb: 17.4, min_vram_gb: 20.0,
            categories: vec!["通用".into(), "多模态".into()],
            quality: 9, description: "Gemma 3 顶配，24GB 卡可用，多模态".into(),
            ollama_cmd: "ollama pull gemma3:27b".into(), speed_rating: "slow".into(),
            family: "Gemma".into(), generation: 3,
            evidence_tier: EvidenceTier::BaseModel, confidence: 0.6,
            is_moe: false, active_params_gb: None,
        },
        ModelEntry {
            name: "qwen3-coder:30b".into(), display_name: "Qwen3 Coder 30B".into(),
            size_gb: 18.6, min_vram_gb: 22.0,
            categories: vec!["代码".into(), "智能体".into()],
            quality: 9, description: "MoE 只激活 3.3B，24GB 显卡上性价比最高的代码模型，256K 上下文".into(),
            ollama_cmd: "ollama pull qwen3-coder:30b".into(), speed_rating: "medium".into(),
            family: "Qwen".into(), generation: 3,
            evidence_tier: EvidenceTier::Variant, confidence: 0.7,
            is_moe: true, active_params_gb: Some(3.3),
        },
        ModelEntry {
            name: "deepseek-r1:32b".into(), display_name: "DeepSeek R1 32B".into(),
            size_gb: 19.9, min_vram_gb: 24.0,
            categories: vec!["推理".into(), "代码".into()],
            quality: 9, description: "R1 蒸馏最大档，复杂推理接近原版".into(),
            ollama_cmd: "ollama pull deepseek-r1:32b".into(), speed_rating: "slow".into(),
            family: "DeepSeek".into(), generation: 3,
            evidence_tier: EvidenceTier::Variant, confidence: 0.7,
            is_moe: false, active_params_gb: None,
        },
        ModelEntry {
            name: "qwen2.5-coder:32b".into(), display_name: "Qwen2.5 Coder 32B".into(),
            size_gb: 19.9, min_vram_gb: 24.0,
            categories: vec!["代码".into()],
            quality: 9, description: "稠密代码模型的天花板，补全质量极稳".into(),
            ollama_cmd: "ollama pull qwen2.5-coder:32b".into(), speed_rating: "slow".into(),
            family: "Qwen".into(), generation: 2,
            evidence_tier: EvidenceTier::Variant, confidence: 0.7,
            is_moe: false, active_params_gb: None,
        },
        ModelEntry {
            name: "qwen3:32b".into(), display_name: "Qwen3 32B".into(),
            size_gb: 20.2, min_vram_gb: 24.0,
            categories: vec!["通用".into(), "中文".into(), "代码".into()],
            quality: 9, description: "千问 32B，24GB 高端消费卡的通用首选".into(),
            ollama_cmd: "ollama pull qwen3:32b".into(), speed_rating: "slow".into(),
            family: "Qwen".into(), generation: 3,
            evidence_tier: EvidenceTier::BaseModel, confidence: 0.6,
            is_moe: false, active_params_gb: None,
        },
        // ── 超大：需要专业卡或大统一内存 ──
        ModelEntry {
            name: "llama3.3:70b".into(), display_name: "Llama 3.3 70B".into(),
            size_gb: 42.5, min_vram_gb: 48.0,
            categories: vec!["通用".into(), "英文".into()],
            quality: 9, description: "要双卡或统一内存的大机器，英文能力顶级".into(),
            ollama_cmd: "ollama pull llama3.3:70b".into(), speed_rating: "slow".into(),
            family: "Llama".into(), generation: 3,
            evidence_tier: EvidenceTier::BaseModel, confidence: 0.6,
            is_moe: false, active_params_gb: None,
        },
        ModelEntry {
            name: "gpt-oss:120b".into(), display_name: "GPT-OSS 120B".into(),
            size_gb: 65.4, min_vram_gb: 72.0,
            categories: vec!["通用".into(), "推理".into(), "代码".into()],
            quality: 10, description: "OpenAI 开放权重旗舰，MoE 激活 5.1B；需要 80GB 级显存或大统一内存".into(),
            ollama_cmd: "ollama pull gpt-oss:120b".into(), speed_rating: "slow".into(),
            family: "GPT-OSS".into(), generation: 1,
            evidence_tier: EvidenceTier::BaseModel, confidence: 0.6,
            is_moe: true, active_params_gb: Some(5.1),
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

/// 为「本机已装、但目录里没有」的模型造一条记录。
///
/// 用户自己 `ollama pull` 过的东西不该在这一页凭空消失。体积和显存需求未知，
/// 标成「已安装」并给最高证据等级——它就在这台机器上跑着，这是最硬的证据。
pub fn entry_for_installed(name: &str, _hw: &HardwareInfo) -> ModelRecommendation {
    let model = ModelEntry {
        name: name.to_string(),
        display_name: name.to_string(),
        size_gb: 0.0,
        min_vram_gb: 0.0,
        categories: vec!["本机已装".into()],
        quality: 0,
        description: "本机已经装了这个模型（不在推荐目录里）。".into(),
        ollama_cmd: format!("ollama pull {name}"),
        speed_rating: "unknown".into(),
        family: "local".into(),
        generation: 0,
        evidence_tier: EvidenceTier::Direct,
        confidence: 1.0,
        is_moe: false,
        active_params_gb: None,
    };
    ModelRecommendation {
        install_cmd: model.ollama_cmd.clone(),
        confidence_label: format!("{} {}", model.evidence_tier.icon(), model.evidence_tier.label()),
        model,
        // 已经跑在这台机器上了，不必再判断跑不跑得动。
        fits_vram: true,
        fits_ram: true,
        overall_fit: "perfect".to_string(),
        effective_quality: 0.0,
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    /// 目录里每一条都要能装。「本地模型选型」的下载按钮就是从 `ollama_cmd`
    /// 里抠出模型标签的——缺一条就是一个点不动的按钮。
    #[test]
    fn every_model_carries_a_usable_install_command() {
        for model in get_model_database() {
            assert!(
                model.ollama_cmd.starts_with("ollama pull "),
                "{} 没有可用的安装命令：{:?}",
                model.name,
                model.ollama_cmd
            );
            let tag = model.ollama_cmd.trim_start_matches("ollama pull ").trim();
            assert!(!tag.is_empty(), "{} 的安装命令没带模型标签", model.name);
            assert!(
                !tag.contains(' '),
                "{} 的标签含空格，前端抠出来会是半截：{tag}",
                model.name
            );
        }
    }

    /// 显存要求不能是 0——那会让「跑不动」的判断永远为假，推荐一台机器根本
    /// 带不动的模型。
    #[test]
    fn every_model_declares_a_memory_requirement() {
        for model in get_model_database() {
            assert!(model.min_vram_gb > 0.0, "{} 没写显存需求", model.name);
            assert!(model.size_gb > 0.0, "{} 没写体积", model.name);
            // 权重之外还要放 KV cache 和上下文。显存需求不高于体积，就等于
            // 把「刚好装不下」判成「能跑」。
            assert!(
                model.min_vram_gb > model.size_gb,
                "{} 的显存需求（{}）不比体积（{}）大，留不出上下文的余量",
                model.name,
                model.min_vram_gb,
                model.size_gb
            );
        }
    }

    /// 安装命令里的标签必须和 `name` 一模一样。
    ///
    /// 两处分别写就会分叉：界面上显示的是 `name`，实际下载的是 `ollama_cmd` 里
    /// 那个标签，对不上时用户装到的和他挑的不是同一个模型。
    #[test]
    fn the_install_tag_is_the_model_name_itself() {
        for model in get_model_database() {
            let tag = model.ollama_cmd.trim_start_matches("ollama pull ").trim();
            assert_eq!(tag, model.name, "{} 的安装标签和名字对不上", model.display_name);
        }
    }

    /// 目录里的每个标签在 Ollama registry 上真的存在吗？
    ///
    /// 这个必须联网，所以默认不跑（`cargo test --lib -- --ignored`）。加它的原因
    /// 是上一版目录里躺着 `phi-4:4b`——registry 返回 404，那一条从写下来起就只能
    /// 下载失败，而任何离线检查都发现不了。
    #[tokio::test]
    #[ignore = "要联网：核对每个 tag 在 Ollama registry 上确实存在"]
    async fn every_tag_exists_on_the_ollama_registry() {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(20))
            .build()
            .expect("client");
        let mut missing = Vec::new();
        for model in builtin_model_database() {
            let (name, tag) = model.name.split_once(':').unwrap_or((&model.name, "latest"));
            let url = format!("https://registry.ollama.ai/v2/library/{name}/manifests/{tag}");
            match client.get(&url).send().await {
                Ok(response) if response.status().is_success() => {}
                Ok(response) => missing.push(format!("{} → {}", model.name, response.status())),
                Err(error) => missing.push(format!("{} → 请求失败 {error}", model.name)),
            }
        }
        assert!(missing.is_empty(), "这些标签在 Ollama 上不存在：{missing:#?}");
    }

    /// 显卡数据库拿来做「换张卡能跑什么」的模拟，显存为 0 的条目会让模拟结果
    /// 全是「跑不动」。
    #[test]
    fn the_gpu_database_is_usable_for_simulation() {
        let gpus = get_gpu_database();
        assert!(!gpus.is_empty(), "显卡库是空的，模拟功能没有可选项");
        for gpu in &gpus {
            assert!(gpu.vram_mb > 0, "{} 显存为 0", gpu.name);
        }
        // 随便挑一张卡，模拟必须能出结果。
        let sample = &gpus[0];
        let recs = recommend_for_gpu(&sample.name).expect("已知显卡应当能模拟");
        assert!(!recs.is_empty(), "{} 模拟不出任何推荐", sample.name);
    }
}

#[cfg(test)]
mod catalog_tests {
    use super::*;

    /// 远程目录必须能被内置副本原样表达——否则「远程 JSON」这条路是假的：
    /// 格式对不上，拉回来也解析不了。这条同时把 `resources/model-catalog.json`
    /// 的内容钉死为内置副本的序列化结果。
    #[test]
    fn the_builtin_catalog_round_trips_through_the_remote_json_shape() {
        let builtin = builtin_model_database();
        let json = serde_json::to_string(&builtin).expect("内置目录应当可序列化");
        let parsed: Vec<ModelEntry> =
            serde_json::from_str(&json).expect("远程目录用的就是这个形状");
        assert_eq!(parsed.len(), builtin.len());
        assert_eq!(parsed[0].name, builtin[0].name);
        assert_eq!(parsed[0].ollama_cmd, builtin[0].ollama_cmd);
    }

    /// 仓库里那份 JSON 样板要和内置副本一致，否则第一次远程更新就会让用户
    /// 看到一份和内置不同的目录，而没人知道差在哪。
    #[test]
    fn the_shipped_json_matches_the_builtin_catalog() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("项目根")
            .join("resources/model-catalog.json");
        let raw = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("读不到 {}：{e}", path.display()));
        let shipped: Vec<ModelEntry> =
            serde_json::from_str(&raw).expect("样板 JSON 应当能解析成 ModelEntry");
        let builtin = builtin_model_database();
        assert_eq!(
            shipped.len(),
            builtin.len(),
            "样板 JSON 和内置目录条数对不上，远程更新会悄悄换掉用户看到的列表"
        );
        for (a, b) in shipped.iter().zip(builtin.iter()) {
            assert_eq!(a.name, b.name);
            assert_eq!(a.ollama_cmd, b.ollama_cmd);
        }
    }
}

#[cfg(test)]
mod catalog_export {
    use super::*;

    /// 从内置副本导出 `resources/model-catalog.json`。
    /// 改了内置目录之后跑一次：`cargo test --lib export_catalog -- --ignored`
    #[test]
    #[ignore = "手动跑：改完内置目录后重新导出样板 JSON"]
    fn export_catalog() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("项目根")
            .join("resources/model-catalog.json");
        std::fs::create_dir_all(path.parent().expect("resources 目录")).expect("建目录");
        let json = serde_json::to_string_pretty(&builtin_model_database()).expect("序列化");
        std::fs::write(&path, format!("{json}\n")).expect("写入");
        println!("已导出 {}", path.display());
    }
}

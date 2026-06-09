//! Model Knowledge Base (Odysseus Cookbook + AingDesk inspired)
//!
//! Provides hardware-aware model recommendations.
//! Contains a built-in database of popular AI models with their
//! resource requirements, capabilities, and quality ratings.

use serde::{Deserialize, Serialize};

/// A known model with its requirements and capabilities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelEntry {
    pub name: String,
    pub display_name: String,
    pub size_gb: f32,
    pub min_vram_gb: f32,
    pub categories: Vec<String>,
    pub quality: u32,          // 1-10
    pub description: String,
    pub ollama_cmd: String,
    pub speed_rating: String,  // "fast" | "medium" | "slow"
}

/// GPU information detected from the system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuInfo {
    pub name: String,
    pub vram_mb: u32,
    pub vendor: String,  // "NVIDIA" | "AMD" | "Intel" | "Unknown"
}

/// Hardware scan result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HardwareInfo {
    pub gpu: Option<GpuInfo>,
    pub ram_mb: u32,
    pub cpu_cores: u32,
    pub os: String,
}

/// Model recommendation with fitness info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelRecommendation {
    pub model: ModelEntry,
    pub fits_vram: bool,
    pub fits_ram: bool,
    pub overall_fit: String,  // "perfect" | "tight" | "impossible"
    pub install_cmd: String,
}

/// Get the built-in model knowledge base
pub fn get_model_database() -> Vec<ModelEntry> {
    vec![
        // ── Small models (2-5GB) ──
        ModelEntry {
            name: "qwen2.5:3b".into(),
            display_name: "Qwen 2.5 3B".into(),
            size_gb: 2.0,
            min_vram_gb: 4.0,
            categories: vec!["通用".into(), "中文".into()],
            quality: 6,
            description: "通义千问轻量版，中文对话能力不错，适合简单任务".into(),
            ollama_cmd: "ollama pull qwen2.5:3b".into(),
            speed_rating: "fast".into(),
        },
        ModelEntry {
            name: "llama3.2:3b".into(),
            display_name: "Llama 3.2 3B".into(),
            size_gb: 2.0,
            min_vram_gb: 4.0,
            categories: vec!["通用".into(), "英文".into()],
            quality: 6,
            description: "Meta 轻量模型，英文对话流畅，速度快".into(),
            ollama_cmd: "ollama pull llama3.2:3b".into(),
            speed_rating: "fast".into(),
        },
        ModelEntry {
            name: "phi-4:4b".into(),
            display_name: "Phi-4 4B".into(),
            size_gb: 2.5,
            min_vram_gb: 4.0,
            categories: vec!["推理".into(), "代码".into()],
            quality: 7,
            description: "微软 Phi-4，推理能力强，代码能力不错".into(),
            ollama_cmd: "ollama pull phi-4:4b".into(),
            speed_rating: "fast".into(),
        },
        // ── Medium models (5-10GB) ──
        ModelEntry {
            name: "qwen2.5:7b".into(),
            display_name: "Qwen 2.5 7B".into(),
            size_gb: 4.7,
            min_vram_gb: 6.0,
            categories: vec!["通用".into(), "中文".into(), "代码".into()],
            quality: 8,
            description: "通义千问 7B，中英文优秀，代码能力强，性价比最高".into(),
            ollama_cmd: "ollama pull qwen2.5:7b".into(),
            speed_rating: "medium".into(),
        },
        ModelEntry {
            name: "deepseek-r1:8b".into(),
            display_name: "DeepSeek R1 8B".into(),
            size_gb: 4.9,
            min_vram_gb: 6.0,
            categories: vec!["推理".into(), "代码".into()],
            quality: 8,
            description: "DeepSeek R1 蒸馏版，思维链推理能力强".into(),
            ollama_cmd: "ollama pull deepseek-r1:8b".into(),
            speed_rating: "medium".into(),
        },
        ModelEntry {
            name: "llama3.1:8b".into(),
            display_name: "Llama 3.1 8B".into(),
            size_gb: 4.7,
            min_vram_gb: 6.0,
            categories: vec!["通用".into(), "英文".into()],
            quality: 7,
            description: "Meta Llama 3.1 8B，英文通用能力强".into(),
            ollama_cmd: "ollama pull llama3.1:8b".into(),
            speed_rating: "medium".into(),
        },
        ModelEntry {
            name: "codellama:7b".into(),
            display_name: "Code Llama 7B".into(),
            size_gb: 3.8,
            min_vram_gb: 6.0,
            categories: vec!["代码".into()],
            quality: 7,
            description: "Meta 代码专精模型，代码生成和补全".into(),
            ollama_cmd: "ollama pull codellama:7b".into(),
            speed_rating: "medium".into(),
        },
        ModelEntry {
            name: "gemma3:4b".into(),
            display_name: "Gemma 3 4B".into(),
            size_gb: 3.0,
            min_vram_gb: 5.0,
            categories: vec!["通用".into(), "多模态".into()],
            quality: 7,
            description: "Google Gemma 3，支持图片理解，多模态能力".into(),
            ollama_cmd: "ollama pull gemma3:4b".into(),
            speed_rating: "fast".into(),
        },
        // ── Large models (10-20GB) ──
        ModelEntry {
            name: "qwen2.5:14b".into(),
            display_name: "Qwen 2.5 14B".into(),
            size_gb: 8.9,
            min_vram_gb: 12.0,
            categories: vec!["通用".into(), "中文".into(), "代码".into()],
            quality: 9,
            description: "通义千问 14B，接近 GPT-4 水平".into(),
            ollama_cmd: "ollama pull qwen2.5:14b".into(),
            speed_rating: "slow".into(),
        },
        ModelEntry {
            name: "deepseek-coder-v2:16b".into(),
            display_name: "DeepSeek Coder V2 16B".into(),
            size_gb: 9.0,
            min_vram_gb: 12.0,
            categories: vec!["代码".into()],
            quality: 9,
            description: "DeepSeek 代码专精模型，代码能力极强".into(),
            ollama_cmd: "ollama pull deepseek-coder-v2:16b".into(),
            speed_rating: "slow".into(),
        },
        ModelEntry {
            name: "mistral-nemo:12b".into(),
            display_name: "Mistral Nemo 12B".into(),
            size_gb: 7.1,
            min_vram_gb: 10.0,
            categories: vec!["通用".into(), "英文".into()],
            quality: 8,
            description: "Mistral Nemo，英文通用能力强，函数调用支持好".into(),
            ollama_cmd: "ollama pull mistral-nemo:12b".into(),
            speed_rating: "medium".into(),
        },
        // ── Very large models (20GB+) ──
        ModelEntry {
            name: "qwen2.5:32b".into(),
            display_name: "Qwen 2.5 32B".into(),
            size_gb: 20.0,
            min_vram_gb: 24.0,
            categories: vec!["通用".into(), "中文".into(), "代码".into()],
            quality: 9,
            description: "通义千问 32B，高端消费级显卡可用".into(),
            ollama_cmd: "ollama pull qwen2.5:32b".into(),
            speed_rating: "slow".into(),
        },
        ModelEntry {
            name: "llama3.1:70b".into(),
            display_name: "Llama 3.1 70B".into(),
            size_gb: 40.0,
            min_vram_gb: 48.0,
            categories: vec!["通用".into(), "英文".into()],
            quality: 10,
            description: "Meta Llama 3.1 70B，顶级开源模型".into(),
            ollama_cmd: "ollama pull llama3.1:70b".into(),
            speed_rating: "slow".into(),
        },
    ]
}

/// Detect hardware information
pub fn detect_hardware() -> HardwareInfo {
    let os = if cfg!(windows) { "Windows" } else if cfg!(target_os = "macos") { "macOS" } else { "Linux" };

    // Detect GPU via nvidia-smi (NVIDIA only, most common for AI)
    let gpu = detect_gpu();

    // Detect RAM
    let ram_mb = detect_ram_mb();

    // Detect CPU cores
    let cpu_cores = std::thread::available_parallelism()
        .map(|n| n.get() as u32)
        .unwrap_or(4);

    HardwareInfo { gpu, ram_mb, cpu_cores, os: os.to_string() }
}

/// Detect NVIDIA GPU via nvidia-smi
fn detect_gpu() -> Option<GpuInfo> {
    let output = std::process::Command::new("nvidia-smi")
        .args(["--query-gpu=name,memory.total", "--format=csv,noheader,nounits"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let line = stdout.lines().next()?;
    let parts: Vec<&str> = line.split(',').collect();
    if parts.len() < 2 {
        return None;
    }

    let name = parts[0].trim().to_string();
    let vram_mb: u32 = parts[1].trim().parse().ok()?;

    Some(GpuInfo {
        name,
        vram_mb,
        vendor: "NVIDIA".into(),
    })
}

/// Detect total RAM in MB
fn detect_ram_mb() -> u32 {
    #[cfg(windows)]
    {
        // Windows: use GlobalMemoryStatusEx via PowerShell
        std::process::Command::new("powershell")
            .args(["-Command", "(Get-CimInstance Win32_OperatingSystem).TotalVisibleMemorySize"])
            .output()
            .ok()
            .and_then(|o| String::from_utf8_lossy(&o.stdout).trim().parse::<u32>().ok())
            .map(|kb| kb / 1024)
            .unwrap_or(16384) // default 16GB
    }
    #[cfg(not(windows))]
    {
        // Linux: read /proc/meminfo
        std::fs::read_to_string("/proc/meminfo")
            .ok()
            .and_then(|s| {
                s.lines()
                    .find(|l| l.starts_with("MemTotal:"))
                    .and_then(|l| l.split_whitespace().nth(1))
                    .and_then(|v| v.parse::<u32>().ok())
            })
            .map(|kb| kb / 1024)
            .unwrap_or(16384)
    }
}

/// Get model recommendations based on hardware
pub fn recommend_models(hw: &HardwareInfo) -> Vec<ModelRecommendation> {
    let db = get_model_database();
    let vram = hw.gpu.as_ref().map(|g| g.vram_mb as f32 / 1024.0).unwrap_or(0.0);
    let ram = hw.ram_mb as f32 / 1024.0;

    let mut recommendations: Vec<ModelRecommendation> = db.into_iter().map(|model| {
        let fits_vram = vram >= model.min_vram_gb;
        let fits_ram = ram >= model.size_gb * 1.5; // Need 1.5x model size in RAM

        let overall_fit = if fits_vram {
            "perfect".to_string()
        } else if fits_ram {
            "tight".to_string() // Can run on CPU but slow
        } else {
            "impossible".to_string()
        };

        ModelRecommendation {
            install_cmd: model.ollama_cmd.clone(),
            model,
            fits_vram,
            fits_ram,
            overall_fit,
        }
    }).collect();

    // Sort: perfect first, then tight, then impossible
    recommendations.sort_by(|a, b| {
        let order = |s: &str| match s { "perfect" => 0, "tight" => 1, _ => 2 };
        order(&a.overall_fit).cmp(&order(&b.overall_fit))
            .then(b.model.quality.cmp(&a.model.quality))
    });

    recommendations
}

//! Local-model fit ranking: given a memory budget (GPU VRAM
//! for GPU inference, or system RAM for CPU inference), rank a catalog of popular
//! open-weight models by whether they fit and at which quantization. Pure and
//! unit-tested; the `commands/local_models.rs` layer supplies detected hardware.

use serde::Serialize;

/// A popular open-weight model the user could run locally.
pub struct CatalogModel {
    pub name: &'static str,
    pub params_b: f64,
    pub family: &'static str,
}

/// A curated catalog of commonly-run local models (parameter counts in billions).
pub const CATALOG: &[CatalogModel] = &[
    CatalogModel { name: "Qwen2.5-0.5B", params_b: 0.5, family: "Qwen" },
    CatalogModel { name: "Gemma-2-2B", params_b: 2.6, family: "Gemma" },
    CatalogModel { name: "Phi-3.5-mini", params_b: 3.8, family: "Phi" },
    CatalogModel { name: "Qwen2.5-7B", params_b: 7.6, family: "Qwen" },
    CatalogModel { name: "Llama-3.1-8B", params_b: 8.0, family: "Llama" },
    CatalogModel { name: "Mistral-7B", params_b: 7.2, family: "Mistral" },
    CatalogModel { name: "Gemma-2-9B", params_b: 9.2, family: "Gemma" },
    CatalogModel { name: "Qwen2.5-14B", params_b: 14.8, family: "Qwen" },
    CatalogModel { name: "Phi-3-medium-14B", params_b: 14.0, family: "Phi" },
    CatalogModel { name: "Gemma-2-27B", params_b: 27.2, family: "Gemma" },
    CatalogModel { name: "Qwen2.5-32B", params_b: 32.5, family: "Qwen" },
    CatalogModel { name: "Llama-3.3-70B", params_b: 70.6, family: "Llama" },
    CatalogModel { name: "Qwen2.5-72B", params_b: 72.7, family: "Qwen" },
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Quant {
    Q4,
    Q5,
    Q8,
    F16,
}

impl Quant {
    /// Approximate bytes per parameter (weights only) for each quantization.
    pub fn bytes_per_param(self) -> f64 {
        match self {
            Quant::Q4 => 0.55, // ~4.4 bits incl. metadata
            Quant::Q5 => 0.68,
            Quant::Q8 => 1.06,
            Quant::F16 => 2.0,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Quant::Q4 => "Q4",
            Quant::Q5 => "Q5",
            Quant::Q8 => "Q8",
            Quant::F16 => "F16",
        }
    }

    /// Quantizations from most compressed to full precision.
    pub fn all() -> [Quant; 4] {
        [Quant::Q4, Quant::Q5, Quant::Q8, Quant::F16]
    }
}

/// Estimated memory (GB) to load and run a model: weights + ~20% for the KV
/// cache, activations and runtime overhead.
pub fn estimate_memory_gb(params_b: f64, quant: Quant) -> f64 {
    params_b * quant.bytes_per_param() * 1.2
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Fit {
    Fits,
    Tight,
    WontRun,
}

/// Classify how a memory requirement fits a budget: comfortable (≤80%),
/// tight (≤100%), or won't run.
pub fn classify_fit(needed_gb: f64, budget_gb: f64) -> Fit {
    if needed_gb <= budget_gb * 0.8 {
        Fit::Fits
    } else if needed_gb <= budget_gb {
        Fit::Tight
    } else {
        Fit::WontRun
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ModelRecommendation {
    pub name: String,
    pub family: String,
    pub params_b: f64,
    /// The best quantization that fits the budget (or Q4 if nothing fits).
    pub best_quant: String,
    pub needed_gb: f64,
    pub fit: Fit,
}

/// Rank the catalog for a memory budget (GB): for each model pick the highest-
/// quality quant that fits (falling back to Q4), classify the fit, and order so
/// the largest models that still fit come first, then the rest by size.
pub fn rank_models(budget_gb: f64) -> Vec<ModelRecommendation> {
    let mut out: Vec<ModelRecommendation> = CATALOG
        .iter()
        .map(|model| {
            // Highest-quality quant that comfortably fits, else the smallest (Q4).
            let mut chosen = Quant::Q4;
            for quant in Quant::all() {
                if estimate_memory_gb(model.params_b, quant) <= budget_gb * 0.8 {
                    chosen = quant;
                }
            }
            let needed_gb = estimate_memory_gb(model.params_b, chosen);
            ModelRecommendation {
                name: model.name.to_string(),
                family: model.family.to_string(),
                params_b: model.params_b,
                best_quant: chosen.label().to_string(),
                needed_gb: (needed_gb * 10.0).round() / 10.0,
                fit: classify_fit(needed_gb, budget_gb),
            }
        })
        .collect();
    // Fitting models first (largest first), then non-fitting (smallest first).
    out.sort_by(|a, b| {
        let a_ok = a.fit != Fit::WontRun;
        let b_ok = b.fit != Fit::WontRun;
        b_ok.cmp(&a_ok).then_with(|| {
            if a_ok {
                b.params_b.partial_cmp(&a.params_b).unwrap_or(std::cmp::Ordering::Equal)
            } else {
                a.params_b.partial_cmp(&b.params_b).unwrap_or(std::cmp::Ordering::Equal)
            }
        })
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_estimate_scales_with_params_and_quant() {
        // 8B at Q4 ≈ 8 * 0.55 * 1.2 ≈ 5.28 GB.
        let q4 = estimate_memory_gb(8.0, Quant::Q4);
        assert!((q4 - 5.28).abs() < 0.01);
        // F16 needs much more than Q4.
        assert!(estimate_memory_gb(8.0, Quant::F16) > q4 * 3.0);
    }

    #[test]
    fn fit_tiers() {
        assert_eq!(classify_fit(5.0, 10.0), Fit::Fits); // 50%
        assert_eq!(classify_fit(9.0, 10.0), Fit::Tight); // 90%
        assert_eq!(classify_fit(11.0, 10.0), Fit::WontRun);
    }

    #[test]
    fn ranking_prefers_largest_fitting_and_flags_too_big() {
        // 24 GB (e.g. RTX 4090) — 70B won't fit even at Q4 (~46GB), 32B fits Q4.
        let ranked = rank_models(24.0);
        assert!(!ranked.is_empty());
        // First entry fits and is one of the larger models that fit.
        assert_ne!(ranked[0].fit, Fit::WontRun);
        // 70B/72B are present but flagged WontRun on 24GB.
        let big = ranked.iter().find(|m| m.params_b >= 70.0).unwrap();
        assert_eq!(big.fit, Fit::WontRun);
        // A tiny model comfortably fits.
        let tiny = ranked.iter().find(|m| m.params_b < 1.0).unwrap();
        assert_eq!(tiny.fit, Fit::Fits);
    }

    #[test]
    fn tiny_budget_still_ranks_smallest_first_among_nonfitting() {
        let ranked = rank_models(1.0); // 1 GB — almost nothing fits comfortably.
        // The last entries (won't run) should be ordered smallest→largest.
        let wont: Vec<f64> = ranked.iter().filter(|m| m.fit == Fit::WontRun).map(|m| m.params_b).collect();
        assert!(wont.windows(2).all(|w| w[0] <= w[1]));
    }
}

/** Auto-split from tauri-api.ts — domain: platform. Import via "@/lib/tauri-api". */
import { invoke } from "@tauri-apps/api/core";
import type {
  AgentAccount,
  ModelPlatform,
  PlatformModel,
  HealthCheckDetail,
  PlatformApiKey,
} from "@/types";

// ── Settings ──────────────────────────────────────────

export const settingsApi = {
  get: (key: string) => invoke<string | null>("get_app_setting", { key }),
  set: (key: string, value: string) => invoke("set_app_setting", { key, value }),
  syncExternalConfigs: () => invoke("sync_external_agent_configs"),
};

export const shellApi = {
  pickDirectory: () => invoke<string | null>("pick_directory"),
  pickFile: () => invoke<string | null>("pick_file"),
};

// ── Model Platforms ───────────────────────────────────

export const platformApi = {
  list: () => invoke<ModelPlatform[]>("get_model_platforms"),
  save: (platform: ModelPlatform) => invoke("save_model_platform", { platform }),
  delete: (id: string) => invoke("delete_model_platform", { id }),
  fetchRemoteModels: (platformId: string) => invoke<PlatformModel[]>("fetch_remote_models", { platformId }),
};

// ── Platform Models ───────────────────────────────────

export const modelApi = {
  listByPlatform: (platformId: string) => invoke<PlatformModel[]>("get_platform_models", { platformId }),
  save: (model: PlatformModel) => invoke("save_platform_model", { model }),
  delete: (id: string) => invoke("delete_platform_model", { id }),
  getActive: () => invoke<PlatformModel[]>("get_active_models"),
  getAvailableNames: () => invoke<string[]>("get_available_models"),
  checkStatus: (modelId: string) => invoke<HealthCheckDetail>("check_model_status", { modelId }),
  batchCheck: (platformId: string) => invoke<PlatformModel[]>("batch_check_models", { platformId }),
  reinferCapabilities: (opts: { modelId?: string; platformId?: string }) => invoke<number>("reinfer_model_capabilities", opts),
};

export interface DistillationCandidate {
  id: string;
  conversation_id: string;
  workspace_path: string;
  candidate_type: "memory" | "skill" | "protocol";
  title: string;
  summary: string;
  payload_json: string;
  evidence_json: string;
  model_id: string;
  status: "pending" | "approved" | "rejected";
  created_at: string;
  reviewed_at: string | null;
}

export const distillationApi = {
  generate: (conversationId: string, modelId: string) =>
    invoke<DistillationCandidate[]>("distill_conversation_to_inbox", { conversationId, modelId }),
  /** Distill an external/pre-existing workspace folder from its .omx/development records. */
  generateFromWorkspace: (workspacePath: string, modelId: string) =>
    invoke<DistillationCandidate[]>("distill_workspace_to_inbox", { workspacePath, modelId }),
  list: (status: "pending" | "approved" | "rejected" | "all" = "pending") =>
    invoke<DistillationCandidate[]>("list_distillation_inbox", { status }),
  review: (candidateId: string, approved: boolean) =>
    invoke<DistillationCandidate>("review_distillation_candidate", { candidateId, approved }),
};

// ── Evolution loop — experience auto-injected back into every agent ──
export interface LessonsInfo { count: number; content: string; }
export const evolutionApi = {
  /** Preview the memory block OMNIX auto-injects into agents' context files (CLAUDE.md/AGENTS.md). */
  preview: (workspacePath?: string) =>
    invoke<LessonsInfo>("get_lessons_preview", { workspacePath }),
  /** Embed experience memories lacking an embedding so injection can rank by relevance. */
  reindex: () => invoke<number>("reindex_memory_embeddings", {}),
  /** Merge near-duplicate memories (requires embeddings). Returns merged count. */
  consolidate: () => invoke<number>("consolidate_memories"),
  /** Cache a workspace's embedding/signals for relevance scoring. */
  refreshWorkspace: (workspacePath: string) =>
    invoke<boolean>("refresh_workspace_profile", { workspacePath }),
};

// ── Platform API Keys (multi-key, encrypted) ──────────

export const apiKeyApi = {
  add: (platformId: string, key: string, label?: string) => invoke<PlatformApiKey>("add_platform_api_key", { platformId, key, label }),
  list: (platformId: string) => invoke<PlatformApiKey[]>("list_platform_api_keys", { platformId }),
  select: (keyId: string) => invoke("select_platform_api_key", { keyId }),
  delete: (keyId: string) => invoke("delete_platform_api_key", { keyId }),
  reveal: (keyId: string) => invoke<string>("reveal_platform_api_key", { keyId }),
};

// ── Agent Accounts ────────────────────────────────────

export const accountApi = {
  list: () => invoke<AgentAccount[]>("get_agent_accounts"),
  save: (account: AgentAccount) => invoke("save_agent_account", { account }),
  switch: (id: string) => invoke("switch_agent_account", { id }),
  delete: (id: string) => invoke("delete_agent_account", { id }),
};

// F1: unified per-agent upstream account switcher (OAuth + api-key)
export interface UpstreamAccountOption {
  account_ref: string; kind: "oauth" | "apikey"; label: string;
  provider: string | null; expired: boolean; is_active: boolean;
}
export const upstreamAccountApi = {
  list: (agentName: string) =>
    invoke<UpstreamAccountOption[]>("list_agent_upstream_accounts", { agentName }),
  setActive: (agentName: string, accountRef: string) =>
    invoke<void>("set_active_upstream_account", { agentName, accountRef }),
  getActive: (agentName: string) =>
    invoke<string>("get_active_upstream_account", { agentName }),
};

// F-C: local model fit ranking
export interface HardwareInfo { cpu_cores: number; cpu_brand: string; ram_gb: number; }
export interface ModelRecommendation {
  name: string; family: string; params_b: number; best_quant: string;
  needed_gb: number; fit: "fits" | "tight" | "wont_run";
}
export const localModelApi = {
  detectHardware: () => invoke<HardwareInfo>("detect_hardware"),
  recommend: (budgetGb: number) =>
    invoke<ModelRecommendation[]>("recommend_local_models", { budgetGb }),
};

// ── Remote Dev (Labs) ──

export interface SshHost {
  id: string;
  name: string;
  host: string;
  port: number;
  user: string;
  key_path: string;
  default_workdir: string;
}
export interface SshTestResult {
  ok: boolean;
  latency_ms: number;
  uname: string;
  error: string;
}
export interface RemoteHardware {
  gpu: string;
  ram_mb: number;
  cpu_cores: number;
}
export interface RemoteAgentStatus {
  agent: string;
  bin: string;
  installed: boolean;
  path: string;
  version: string;
}
export interface RemoteModelHostTest {
  ok: boolean;
  latency_ms: number;
  models: string[];
  error: string;
}
export const remoteDevApi = {
  listHosts: () => invoke<SshHost[]>("list_ssh_hosts"),
  saveHost: (host: SshHost) => invoke<SshHost>("save_ssh_host", { host }),
  deleteHost: (id: string) => invoke<void>("delete_ssh_host", { id }),
  testHost: (id: string) => invoke<SshTestResult>("test_ssh_host", { id }),
  probeHardware: (id: string) => invoke<RemoteHardware>("probe_remote_hardware", { id }),
  detectAgents: (id: string) => invoke<RemoteAgentStatus[]>("detect_remote_agents", { id }),
  installAgent: (id: string, agent: string) =>
    invoke<string>("install_remote_agent", { id, agent }),
  testModelHost: (url: string) =>
    invoke<RemoteModelHostTest>("test_remote_model_host", { url }),
  startRun: (hostId: string, agent: string, workdir: string, prompt: string, useGateway: boolean) =>
    invoke<{ run_id: string }>("start_remote_run", { hostId, agent, workdir, prompt, useGateway }),
  stopRun: (runId: string) => invoke<void>("stop_remote_run", { runId }),
};


export type EvidenceTier = "Direct" | "Variant" | "BaseModel" | "LineInterp" | "SelfReported";
export interface ModelEntry {
  name: string; display_name: string; size_gb: number; min_vram_gb: number;
  categories: string[]; quality: number; description: string; ollama_cmd: string; speed_rating: string;
  family: string; generation: number; evidence_tier: EvidenceTier; confidence: number;
  is_moe: boolean; active_params_gb: number | null;
}
export interface ModelRecommendation {
  model: ModelEntry; fits_vram: boolean; fits_ram: boolean;
  overall_fit: "perfect" | "tight" | "impossible"; install_cmd: string;
  effective_quality: number; confidence_label: string;
}
export interface HardwareInfo {
  gpu: { name: string; vram_mb: number; vendor: string } | null;
  ram_mb: number; cpu_cores: number; os: string;
}
export interface GpuSpec {
  name: string; vram_mb: number; bandwidth_gb_s: number; vendor: string; generation: string;
}
export const cookbookApi = {
  getRecommendations: () => invoke<{ hardware: HardwareInfo; recommendations: ModelRecommendation[] }>("get_model_recommendations"),
  getDatabase: () => invoke<ModelEntry[]>("get_model_database"),
  /** Simulate recommendations for a hypothetical GPU */
  recommendForGpu: (gpuName: string) => invoke<{ gpu: GpuSpec | null; recommendations: ModelRecommendation[] }>("recommend_for_gpu", { gpuName }),
  /** Get full GPU database */
  getGpuDatabase: () => invoke<GpuSpec[]>("get_gpu_database"),
};

// Code Deep Analysis

export interface AgentPlatformBinding {
  agent_name: string; platform_id: string; platform_name: string;
  model_name: string | null; binding_kind: "default" | "builtin" | "omnix";
  builtin_model: string | null; enabled: boolean;
}
export const agentBindingApi = {
  getAll: () => invoke<AgentPlatformBinding[]>("get_agent_bindings"),
  set: (
    agentName: string,
    platformId: string,
    modelName?: string,
    bindingKind: "builtin" | "omnix" = "omnix",
    builtinModel?: string,
  ) =>
    invoke("set_agent_binding", { agentName, platformId, modelName, bindingKind, builtinModel }),
  setBuiltin: (agentName: string, builtinModel: string) =>
    invoke("set_agent_binding", {
      agentName,
      platformId: "__agent_builtin__",
      modelName: null,
      bindingKind: "builtin",
      builtinModel,
    }),
  remove: (agentName: string) =>
    invoke("remove_agent_binding", { agentName }),
  toggle: (agentName: string) =>
    invoke("toggle_agent_binding", { agentName }),
};

// Circuit Breaker & Session Usage
export type CircuitState = "Closed" | "Open" | "HalfOpen";
export interface CircuitBreakerStatus {
  platform_id: string; state: CircuitState; consecutive_failures: number;
  total_failures: number; total_successes: number;
  last_failure_at: string | null; last_success_at: string | null;
  last_error: string | null; half_open_threshold: number; failure_threshold: number;
}
export const circuitBreakerApi = {
  getStatus: () => invoke<CircuitBreakerStatus[]>("get_circuit_status"),
  reset: (platformId: string) => invoke("reset_circuit_breaker", { platformId }),
  getModelPricing: () => invoke<Record<string, [number, number]>>("get_model_pricing"),
  estimateCost: (model: string, promptTokens: number, completionTokens: number) =>
    invoke<number>("estimate_model_cost", { model, promptTokens, completionTokens }),
};

// OAuth Auth Center — use your subscriptions in agents

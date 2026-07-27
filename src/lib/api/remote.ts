/** Auto-split from tauri-api.ts — domain: remote. Import via "@/lib/tauri-api". */
import { invoke } from "@tauri-apps/api/core";
import type {
  RemoteAccessInfo,
} from "@/types";

// ── Remote Access ─────────────────────────────────────

export interface RemoteClientInfo {
  ip: string;
  /** unix seconds of the device's last authenticated request */
  last_seen: number;
}

export const remoteApi = {
  getInfo: () => invoke<RemoteAccessInfo>("get_remote_access_info"),
  /** Enable/disable LAN binding for remote phone access; restarts the proxy. */
  setAccess: (enabled: boolean) => invoke<void>("set_remote_access", { enabled }),
  /** Mint a fresh token — every previously issued URL/QR stops working. */
  rotateToken: () => invoke<string>("rotate_remote_token"),
  /** Devices that recently authenticated against the remote panel. */
  clients: () => invoke<RemoteClientInfo[]>("get_remote_clients"),
};


export type OAuthProvider = "anthropic_claude" | "openai_codex" | "google_gemini";
export interface OAuthStartResult {
  authorize_url: string; state: string; manual_paste: boolean; redirect_uri: string;
}
export interface OAuthAccountView {
  id: string; provider: OAuthProvider; provider_name: string; label: string;
  scope: string | null; expires_at: string | null; has_refresh: boolean;
  expired: boolean; created_at: string;
}
export const oauthApi = {
  start: (provider: OAuthProvider) => invoke<OAuthStartResult>("oauth_start", { provider }),
  complete: (provider: OAuthProvider, callbackInput: string, label: string) =>
    invoke<OAuthAccountView>("oauth_complete", { provider, callbackInput, label }),
  listAccounts: () => invoke<OAuthAccountView[]>("oauth_list_accounts"),
  deleteAccount: (id: string) => invoke<void>("oauth_delete_account", { id }),
  refreshAccount: (id: string) => invoke<void>("oauth_refresh_account", { id }),
};

// Office 底座 — OfficeCLI managed install + pptx QA/import; skill auto-update.

export interface GrokAuthStatus {
  cli_installed: boolean;
  cli_path: string | null;
  signed_in: boolean;
  auth_file: string;
  api_key_env: boolean;
  api_key_in_omnix: boolean;
}
export interface GrokModel { id: string; name: string; }
export const grokAuthApi = {
  status: () => invoke<GrokAuthStatus>("grok_auth_status"),
  loginStart: () => invoke<void>("grok_login_start"),
  loginCancel: () => invoke<void>("grok_login_cancel"),
  logout: () => invoke<void>("grok_logout"),
  // 探测登录账号可用的 Grok 模型（零 token 握手）——供选中 Grok 时填充选择器。
  availableModels: () => invoke<GrokModel[]>("grok_available_models"),
};

// CLI 配置接管 — point native CLIs at a chosen target
export interface TakeoverTarget { kind: "gateway" | "platform" | "oauth"; ref_id?: string; model?: string; }
export interface TakeoverReport { agent: string; config_path: string; applied: boolean; backup_path: string | null; detail: string; }
export interface AgentTakeoverState { agent: string; config_path: string; config_exists: boolean; current_base_url: string | null; has_backup: boolean; }
export const cliTakeoverApi = {
  status: () => invoke<AgentTakeoverState[]>("cli_takeover_status"),
  apply: (agents: string[], target: TakeoverTarget) =>
    invoke<TakeoverReport[]>("cli_takeover_apply", { agents, target }),
  revert: (agent: string) => invoke<string>("cli_takeover_revert", { agent }),
};

// Skill DAG

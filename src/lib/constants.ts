/**
 * OMNIX DevFlow — Application Constants
 */

/** Developer tips shown on the Dashboard */
export const OMNIX_TIPS = [
  {
    title: "Claude Code 首次启动静默跳过",
    desc: "OMNIX 后台会自动将预先接受的许可条款和 telemetry opt-out 参数写入到 C:\\Users\\87953\\.config\\claude-code\\config.json 中，确保一键免交互首启。",
  },
  {
    title: "利用本地中转网关实现跨模型开发",
    desc: "Claude Code 工具默认锁定 Anthropic API 格式。通过 OMNIX 代理，你可以将 Claude 协议透明中转至 DeepSeek API (deepseek-chat)，在节约 90% 费用的同时保持完整功能。",
  },
  {
    title: "进程空闲守护 (Idle Reaper)",
    desc: "当 Agent (Node.js/Python 进程) 运行完且无会话交互超过设置的时间，OMNIX 后端会优雅地终止其进程，彻底释放显存和内存。",
  },
  {
    title: "原子安全覆写机制 (Atomic Rename)",
    desc: "所有 CLI 软件的配置文件更新皆采用 Rust std::fs 原子写入策略：先向 .tmp 写入配置包并完成内容校验后，再原子性覆写目标文件，绝不损坏配置。",
  },
  {
    title: "本地 LLM 硬件显存适配",
    desc: "开启 GPU 硬件加速开关能加快本地 Ollama Embedding 的向量索引构建，OMNIX 会智能感知显存，并在侧边栏向您推荐适配参数。",
  },
] as const;

/** Default model names shown in model selector dropdowns */
export const DEFAULT_MODEL_NAMES = [
  "claude-3-5-sonnet",
  "deepseek-chat",
  "gpt-4o",
  "gemini-2.0-flash",
  "qwen-plus",
] as const;

/** Supported agent CLI names shown in the agent switcher */
export const AGENT_NAMES = [
  "Claude Code",
  "Gemini CLI",
  "Codex",
  "Qwen Code",
  "GitHub Copilot CLI",
  "Google Antigravity",
  "OpenCode",
] as const;

/** Supported LLM provider types with display labels */
export const PROVIDER_TYPES: Record<string, string> = {
  "openai": "OpenAI (Chat Completions)",
  "openai-response": "OpenAI (Responses API)",
  "anthropic": "Anthropic (Claude)",
  "gemini": "Google Gemini",
  "azure-openai": "Azure OpenAI",
  "ollama": "Ollama (本地)",
  "mistral": "Mistral",
  "new-api": "One API / New API 网关",
  "openai-compatible": "OpenAI 兼容",
} as const;

/** Default cron schedule expression */
export const DEFAULT_CRON_SCHEDULE = "*/15 * * * *";

/** Default proxy port */
export const DEFAULT_PROXY_PORT = "1421";

/** Default idle timeout in minutes */
export const DEFAULT_IDLE_TIMEOUT = "15";

/** Default WSL distribution */
export const DEFAULT_WSL_DISTRO = "Ubuntu";

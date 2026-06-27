/**
 * OMNIX Workbench - Application Constants
 */

/** Canonical product naming. Keep these aligned with AGENTS.md and Tauri metadata. */
export const PRODUCT_NAME = "OMNIX Workbench";
export const PRODUCT_SHORT_NAME = "OMNIX";
export const PRODUCT_DESCRIPTOR_ZH = "多 Agent 开发与协作工作台";
export const PRODUCT_PACKAGE_SLUG = "omnix-workbench";

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

// ══════════════════════════════════════════════════
// API Provider Presets (ZCF inspired)
// ══════════════════════════════════════════════════

export interface ApiProviderPreset {
  id: string;
  name: string;
  api_type: "openai" | "anthropic" | "ollama";
  api_address: string;
  default_model: string;
  description: string;
  region: "global" | "china" | "local";
}

export const API_PROVIDER_PRESETS: ApiProviderPreset[] = [
  // ── Global Providers ──
  {
    id: "openai",
    name: "OpenAI",
    api_type: "openai",
    api_address: "https://api.openai.com/v1",
    default_model: "gpt-4o",
    description: "OpenAI 官方 API",
    region: "global",
  },
  {
    id: "anthropic",
    name: "Anthropic",
    api_type: "anthropic",
    api_address: "https://api.anthropic.com",
    default_model: "claude-sonnet-4-20250514",
    description: "Anthropic 官方 API",
    region: "global",
  },
  {
    id: "openrouter",
    name: "OpenRouter",
    api_type: "openai",
    api_address: "https://openrouter.ai/api/v1",
    default_model: "anthropic/claude-sonnet-4-20250514",
    description: "多模型聚合路由，支持 100+ 模型",
    region: "global",
  },
  // ── China Providers ──
  {
    id: "deepseek",
    name: "DeepSeek",
    api_type: "openai",
    api_address: "https://api.deepseek.com/v1",
    default_model: "deepseek-chat",
    description: "DeepSeek 官方 API，代码能力强",
    region: "china",
  },
  {
    id: "siliconflow",
    name: "硅基流动 SiliconFlow",
    api_type: "openai",
    api_address: "https://api.siliconflow.cn/v1",
    default_model: "Qwen/Qwen2.5-7B-Instruct",
    description: "硅基流动聚合平台，免费模型多",
    region: "china",
  },
  {
    id: "zhipu",
    name: "智谱 GLM",
    api_type: "openai",
    api_address: "https://open.bigmodel.cn/api/paas/v4",
    default_model: "glm-4-flash",
    description: "智谱清言 API，GLM 系列模型",
    region: "china",
  },
  {
    id: "moonshot",
    name: "月之暗面 Kimi",
    api_type: "openai",
    api_address: "https://api.moonshot.cn/v1",
    default_model: "moonshot-v1-8k",
    description: "Kimi 长上下文模型",
    region: "china",
  },
  {
    id: "minimax",
    name: "MiniMax",
    api_type: "openai",
    api_address: "https://api.minimax.chat/v1",
    default_model: "MiniMax-Text-01",
    description: "MiniMax 海螺 AI API",
    region: "china",
  },
  {
    id: "bailian",
    name: "百炼 Bailian",
    api_type: "openai",
    api_address: "https://dashscope.aliyuncs.com/compatible-mode/v1",
    default_model: "qwen-plus",
    description: "阿里云百炼平台，通义千问系列",
    region: "china",
  },
  {
    id: "volcengine",
    name: "火山引擎",
    api_type: "openai",
    api_address: "https://ark.cn-beijing.volces.com/api/v3",
    default_model: "doubao-pro-32k",
    description: "字节跳动火山引擎，豆包系列模型",
    region: "china",
  },
  // ── Local Providers ──
  {
    id: "ollama",
    name: "Ollama (本地)",
    api_type: "ollama",
    api_address: "http://localhost:11434",
    default_model: "qwen2.5:7b",
    description: "本地 Ollama 模型服务",
    region: "local",
  },
  {
    id: "lmstudio",
    name: "LM Studio (本地)",
    api_type: "openai",
    api_address: "http://localhost:1234/v1",
    default_model: "local-model",
    description: "本地 LM Studio 模型服务",
    region: "local",
  },
];

// ══════════════════════════════════════════════════
// MCP Service Presets (ZCF inspired)
// ══════════════════════════════════════════════════

export interface McpServicePreset {
  id: string;
  name: string;
  transport: "stdio" | "sse";
  command?: string;
  args?: string[];
  url?: string;
  description: string;
  requires_key: boolean;
}

export const MCP_SERVICE_PRESETS: McpServicePreset[] = [
  {
    id: "context7",
    name: "Context7",
    transport: "stdio",
    command: "npx",
    args: ["-y", "@upstash/context7-mcp@latest"],
    description: "实时文档查询，获取最新库文档",
    requires_key: false,
  },
  {
    id: "playwright",
    name: "Playwright",
    transport: "stdio",
    command: "npx",
    args: ["@playwright/mcp@latest"],
    description: "浏览器自动化测试和网页交互",
    requires_key: false,
  },
  {
    id: "web-search",
    name: "Web Search",
    transport: "stdio",
    command: "npx",
    args: ["-y", "@modelcontextprotocol/server-brave-search"],
    description: "Brave 搜索引擎集成（需要 BRAVE_API_KEY）",
    requires_key: true,
  },
  {
    id: "filesystem",
    name: "Filesystem",
    transport: "stdio",
    command: "npx",
    args: ["-y", "@modelcontextprotocol/server-filesystem", "."],
    description: "文件系统读写访问",
    requires_key: false,
  },
  {
    id: "deepwiki",
    name: "DeepWiki",
    transport: "sse",
    url: "https://mcp.deepwiki.com/sse",
    description: "深度知识库查询",
    requires_key: false,
  },
  {
    id: "memory",
    name: "Memory",
    transport: "stdio",
    command: "npx",
    args: ["-y", "@modelcontextprotocol/server-memory"],
    description: "持久化记忆存储",
    requires_key: false,
  },
  {
    id: "sequential-thinking",
    name: "Sequential Thinking",
    transport: "stdio",
    command: "npx",
    args: ["-y", "@modelcontextprotocol/server-sequential-thinking"],
    description: "结构化思维链推理",
    requires_key: false,
  },
];

// ══════════════════════════════════════════════════
// Output Style Presets (ZCF inspired)
// ══════════════════════════════════════════════════

export interface OutputStylePreset {
  id: string;
  name: string;
  description: string;
  system_prompt_suffix: string;
}

export const OUTPUT_STYLE_PRESETS: OutputStylePreset[] = [
  {
    id: "engineer-professional",
    name: "专业工程风格",
    description: "严谨、简洁、注重代码质量和技术准确性",
    system_prompt_suffix: "You are a senior software engineer. Be precise, concise, and technically accurate. Always consider edge cases, error handling, and security implications. Prefer standard library solutions over third-party dependencies.",
  },
  {
    id: "concise",
    name: "简洁风格",
    description: "最少文字，直接给答案和代码",
    system_prompt_suffix: "Be extremely concise. No explanations unless asked. Direct code output. Minimal comments. Think step-by-step internally but output only the result.",
  },
  {
    id: "educational",
    name: "教学风格",
    description: "详细解释原理，适合学习场景",
    system_prompt_suffix: "You are a patient teacher. Explain concepts clearly with examples. When writing code, add detailed comments explaining each step. Suggest related topics for further learning. Use analogies when helpful.",
  },
  {
    id: "creative",
    name: "创意风格",
    description: "灵活、开放、善于提出多种方案",
    system_prompt_suffix: "Think creatively and propose multiple approaches to each problem. Consider unconventional solutions. Evaluate trade-offs between simplicity, performance, and maintainability. Be open to experimentation.",
  },
];

/**
 * Nominal context-window sizes (tokens) by model-name substring, for the
 * context-budget meter. These are the models' published windows — used only to
 * show how full the OMNIX-stored transcript is, not a billing figure.
 */
const CONTEXT_WINDOW_TABLE: { match: string; window: number }[] = [
  { match: "claude", window: 200_000 },
  { match: "gpt-5", window: 256_000 },
  { match: "o1", window: 200_000 },
  { match: "o3", window: 200_000 },
  { match: "o4", window: 200_000 },
  { match: "gpt-4.1", window: 1_000_000 },
  { match: "gpt-4o", window: 128_000 },
  { match: "gpt-4", window: 128_000 },
  { match: "deepseek", window: 128_000 },
  { match: "qwen", window: 128_000 },
  { match: "glm", window: 128_000 },
  { match: "doubao", window: 128_000 },
  { match: "moonshot", window: 128_000 },
  { match: "kimi", window: 128_000 },
  { match: "gemini", window: 1_000_000 },
  { match: "llama", window: 128_000 },
];

/** Best-effort context window (tokens) for a model name; defaults to 128K. */
export function contextWindowFor(modelName?: string | null): number {
  if (!modelName) return 128_000;
  const lower = modelName.toLowerCase();
  const hit = CONTEXT_WINDOW_TABLE.find((entry) => lower.includes(entry.match));
  return hit ? hit.window : 128_000;
}

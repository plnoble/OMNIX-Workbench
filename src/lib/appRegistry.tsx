import {
  Bell,
  Bot,
  Brain,
  CalendarClock,
  Code2,
  Database,
  FlaskConical,
  GitCompare,
  Grid3X3,
  Languages,
  MessageSquare,
  Plug,
  Search,
  Sparkles,
  StickyNote,
  UserRound,
  Users,
  Wand2,
  Webhook,
} from "lucide-react";

import type { AppEntry, NavigationLayout } from "@/types";

export const APP_ENTRIES: AppEntry[] = [
  {
    id: "chat",
    label: "对话",
    title: "对话",
    description: "选择一个 Agent 或模型，直接开始普通对话，不绑定工作区。",
    group: "core",
    placement: "pinned",
    is_core: true,
  },
  {
    id: "work",
    label: "工作",
    title: "单 Agent 工作",
    description: "选择一个工作区，让 Agent 在项目里读写文件、处理开发任务。",
    group: "core",
    placement: "pinned",
    is_core: true,
  },
  {
    id: "team",
    label: "团队",
    title: "Team",
    description: "队长拆解计划，确认后启动多个 Worker。",
    group: "core",
    placement: "pinned",
    is_core: true,
  },
  {
    id: "agents",
    label: "智能体",
    title: "Agents",
    description: "检测、安装、更新和配置本地 Agent。",
    group: "core",
    placement: "pinned",
    is_core: true,
  },
  {
    id: "skills",
    label: "技能",
    title: "Skills",
    description: "管理技能包、同步目标和漂移状态。",
    group: "core",
    placement: "pinned",
    is_core: true,
  },
  {
    id: "models",
    label: "Models",
    title: "模型中心",
    description: "供应商、多 API Key、模型列表、能力和健康检查。",
    group: "resource",
    placement: "launcher",
  },
  {
    id: "knowledge",
    label: "Knowledge",
    title: "知识库",
    description: "普通对话可手动选择的 RAG 资料源。",
    group: "resource",
    placement: "launcher",
  },
  {
    id: "memories",
    label: "Memory",
    title: "记忆",
    description: "长期经验、避坑记录和上下文复用。",
    group: "resource",
    placement: "launcher",
  },
  {
    id: "mcp",
    label: "MCP",
    title: "MCP / 工具",
    description: "工具服务、MCP Server 和执行能力。",
    group: "resource",
    placement: "launcher",
  },
  {
    id: "search",
    label: "Search",
    title: "搜索",
    description: "联网搜索供应商和搜索调试。",
    group: "resource",
    placement: "launcher",
  },
  {
    id: "quick-assistant",
    label: "快捷助手",
    title: "划词助手",
    description: "划词后翻译、解释、总结、润色、搜索和复制。",
    group: "assistant",
    placement: "launcher",
  },
  {
    id: "assistants",
    label: "助手",
    title: "助手模板库",
    description: "Agent 角色模板、提示词复制和一键带入工作页。",
    group: "assistant",
    placement: "launcher",
  },
  {
    id: "cron",
    label: "Cron",
    title: "定时任务",
    description: "后台定时执行 Agent 任务。",
    group: "labs",
    placement: "launcher",
    is_experimental: true,
  },
  {
    id: "hooks",
    label: "Hooks",
    title: "事件 Hooks",
    description: "Agent 事件触发的自动化规则：通知 / 执行命令 / 记录。",
    group: "resource",
    placement: "launcher",
  },
  {
    id: "notes",
    label: "笔记",
    title: "笔记",
    description: "本地 Markdown 笔记，可从划词助手一键存入。",
    group: "resource",
    placement: "launcher",
  },
  {
    id: "translate",
    label: "翻译",
    title: "翻译",
    description: "多语言 AI 翻译，源↔目标双栏，支持历史。",
    group: "resource",
    placement: "launcher",
  },
  {
    id: "compare",
    label: "Compare",
    title: "模型对比",
    description: "多模型输出对比和评审。",
    group: "labs",
    placement: "launcher",
    is_experimental: true,
  },
  {
    id: "code-analysis",
    label: "Code Analysis",
    title: "代码分析",
    description: "代码结构分析入口，后续接入验证链路。",
    group: "labs",
    placement: "launcher",
    is_experimental: true,
    is_incomplete: true,
  },
  {
    id: "studio",
    label: "创作",
    title: "创作 Studio",
    description: "文生图与文生视频：连接媒体供应商（Agnes AI 等），统一画廊管理作品。",
    group: "resource",
    placement: "launcher",
  },
  {
    id: "profile",
    label: "画像",
    title: "编程画像",
    description: "你的 AI 编程活动热力图、连续天数与可分享战绩卡。",
    group: "resource",
    placement: "launcher",
  },
  {
    id: "labs",
    label: "Labs",
    title: "实验室",
    description: "未稳定能力总览和风险状态。",
    group: "labs",
    placement: "launcher",
    is_experimental: true,
  },
];

export const DEFAULT_NAVIGATION_LAYOUT: NavigationLayout = {
  pinned: ["chat", "work", "team", "agents", "skills"],
  launcher: [
    "models",
    "knowledge",
    "memories",
    "mcp",
    "search",
    "quick-assistant",
    "assistants",
    "cron",
    "compare",
    "code-analysis",
    "labs",
  ],
  hidden: [],
};

export const APP_ICON_MAP = {
  chat: MessageSquare,
  work: Code2,
  team: Users,
  agents: Bot,
  skills: Sparkles,
  models: Database,
  knowledge: Brain,
  memories: Brain,
  mcp: Plug,
  search: Search,
  "quick-assistant": Bell,
  assistants: Wand2,
  cron: CalendarClock,
  hooks: Webhook,
  notes: StickyNote,
  translate: Languages,
  compare: GitCompare,
  "code-analysis": Code2,
  studio: Wand2,
  profile: UserRound,
  labs: FlaskConical,
  launcher: Grid3X3,
} as const;

export function getAppEntry(id: string): AppEntry | undefined {
  return APP_ENTRIES.find((entry) => entry.id === id);
}

export function normalizeNavigationLayout(layout?: Partial<NavigationLayout> | null): NavigationLayout {
  const knownIds = new Set(APP_ENTRIES.map((entry) => entry.id));
  const seen = new Set<string>();

  const clean = (ids?: string[]) => {
    const result: string[] = [];
    for (const id of ids ?? []) {
      if (!knownIds.has(id) || seen.has(id)) continue;
      seen.add(id);
      result.push(id);
    }
    return result;
  };

  const pinned = clean(layout?.pinned);
  const launcher = clean(layout?.launcher);
  // The "隐藏" tier was removed — fold any previously-hidden apps into the grid.
  for (const id of clean(layout?.hidden)) launcher.push(id);

  for (const entry of APP_ENTRIES) {
    if (!seen.has(entry.id)) {
      if (entry.placement === "pinned") pinned.push(entry.id);
      else launcher.push(entry.id);
      seen.add(entry.id);
    }
  }

  // The two primary surfaces (对话 / 工作) must always stay pinned and lead.
  for (const required of ["work", "chat"]) {
    if (!pinned.includes(required)) {
      const fromLauncher = launcher.indexOf(required);
      if (fromLauncher >= 0) launcher.splice(fromLauncher, 1);
      pinned.unshift(required);
    }
  }

  return { pinned, launcher, hidden: [] };
}

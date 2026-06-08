/**
 * AppHeader — Header ribbon with title and preview toggle
 */

import { Button } from "@/components/ui/button";
import { Eye } from "lucide-react";

interface AppHeaderProps {
  activeTab: string;
  activeAgent: string;
  chatWorkspace: string;
  showPreviewButton: boolean;
  isPreviewOpen: boolean;
  onTogglePreview: () => void;
}

const TAB_TITLES: Record<string, { title: string; subtitle: string }> = {
  dashboard: { title: "📊 开发环境诊断控制面板", subtitle: "" },
  chat: { title: "💬 智能体对话", subtitle: "" },
  agents: { title: "🤖 Agent 仓库 & 管理管理器", subtitle: "" },
  compare: { title: "⚖️ AI 专家比对中枢", subtitle: "支持本地 Ollama、OpenAI 格式端点或透明中转 Anthropic 通信密钥" },
  team: { title: "👥 多智能体多窗口团队协同", subtitle: "团队并行开发，共享同一个 workspace 运行环境" },
  memories: { title: "🧠 长期避坑记忆库与经验蒸馏", subtitle: "" },
  skills: { title: "🧬 自进化技能与融合熔炉", subtitle: "" },
  cron: { title: "⏰ 定时计划与执行监视器", subtitle: "" },
  settings: { title: "⚙️ 模型中转代理与网关设置", subtitle: "支持本地 Ollama、OpenAI 格式端点或透明中转 Anthropic 通信密钥" },
};

export function AppHeader({
  activeTab,
  activeAgent,
  chatWorkspace,
  showPreviewButton,
  isPreviewOpen,
  onTogglePreview,
}: AppHeaderProps) {
  const tabInfo = TAB_TITLES[activeTab] || { title: "", subtitle: "" };

  // Dynamic subtitle for chat tab
  const subtitle = activeTab === "chat"
    ? chatWorkspace === "direct" ? "工作区: 直聊模式" : `工作区: ${chatWorkspace}`
    : tabInfo.subtitle;

  const title = activeTab === "chat"
    ? `💬 智能体对话 (${activeAgent})`
    : tabInfo.title;

  return (
    <header className="border-b border-border glass-panel">
      <div className="flex justify-between items-center">
        <div>
          <h2 className="text-lg font-semibold capitalize m-0">{title}</h2>
          {subtitle && (
            <span className="text-xs text-muted-foreground">{subtitle}</span>
          )}
        </div>

        {showPreviewButton && (
          <Button
            size="sm"
            variant="outline"
            onClick={onTogglePreview}
          >
            <Eye className="h-3 w-3" />
            {isPreviewOpen ? "关闭实时预览" : "开启实时预览"}
          </Button>
        )}
      </div>
    </header>
  );
}

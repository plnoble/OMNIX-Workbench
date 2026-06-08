/**
 * ChatTab — 智能体对话主界面
 *
 * Agent switcher, message list, interactive prompt cards, send bar
 */

import { Card, CardContent } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Textarea } from "@/components/ui/textarea";
import { Send, Square, Shield, ArrowUp, ArrowDown, ArrowLeft, ArrowRight, Check } from "lucide-react";
import { AGENT_NAMES, DEFAULT_MODEL_NAMES } from "@/lib/constants";
import { cn } from "@/lib/utils";
import type { ConversationMessage, DetectedAgent, PlatformModel, PromptType } from "@/types";

interface ChatTabProps {
  activeAgent: string;
  detectedAgents: DetectedAgent[];
  messages: ConversationMessage[];
  chatInput: string;
  chatWorkspace: string;
  currentConvId: string;
  activeSessions: string[];
  promptType: PromptType;
  targetModel: string;
  activeModels: PlatformModel[];
  setActiveAgent: (name: string) => void;
  setChatInput: (val: string) => void;
  setChatWorkspace: (val: string) => void;
  setTargetModel: (val: string) => void;
  onSendMessage: (e: React.FormEvent) => void;
  onSendStdinDirect: (input: string) => void;
  onStopSession: (id: string) => void;
}

export function ChatTab({
  activeAgent,
  detectedAgents,
  messages,
  chatInput,
  chatWorkspace,
  currentConvId,
  activeSessions,
  promptType,
  targetModel,
  activeModels,
  setActiveAgent,
  setChatInput,
  setChatWorkspace,
  setTargetModel,
  onSendMessage,
  onSendStdinDirect,
  onStopSession,
}: ChatTabProps) {
  const modelOptions = buildModelOptions(activeModels, targetModel);

  return (
    <div className="flex flex-col h-full flex-1">
      {/* Agent Switcher */}
      <div className="px-4 py-3 border-b border-border flex gap-2 overflow-x-auto">
        {AGENT_NAMES.map((name) => {
          const agentInfo = detectedAgents.find((a) => a.name === name);
          const isInstalled = agentInfo?.status === "installed";
          const isActive = activeAgent === name;

          return (
            <button
              key={name}
              onClick={() => setActiveAgent(name)}
              className={cn(
                "flex items-center gap-1.5 px-3.5 py-1.5 rounded-full border text-xs cursor-pointer transition-all",
                isActive
                  ? "bg-accent border-accent text-accent-foreground"
                  : "bg-white/[0.02] border-border text-foreground hover:bg-white/5"
              )}
            >
              <span
                className={cn(
                  "w-1.5 h-1.5 rounded-full",
                  isInstalled ? "bg-emerald-500" : "bg-gray-500"
                )}
              />
              {name}
            </button>
          );
        })}
      </div>

      {/* Messages */}
      <div className="flex-1 p-5 overflow-y-auto flex flex-col gap-4">
        {messages.length === 0 ? (
          <EmptyState activeAgent={activeAgent} onQuickPrompt={setChatInput} />
        ) : (
          messages.map((msg) => (
            <div
              key={msg.id}
              className={cn(
                "flex",
                msg.role === "user" ? "justify-end" : "justify-start"
              )}
            >
              <div
                className={cn(
                  "max-w-[70%] rounded-2xl px-4 py-3",
                  msg.role === "user"
                    ? "bg-accent/20 text-foreground"
                    : "bg-white/[0.03] border border-border text-foreground"
                )}
              >
                <div className="text-xs text-muted-foreground mb-1">
                  {msg.role === "user" ? "用户" : activeAgent}
                </div>
                <div className="text-sm leading-relaxed whitespace-pre-wrap">
                  {msg.content}
                </div>
              </div>
            </div>
          ))
        )}

        {/* Interactive Prompt Cards */}
        {promptType !== "none" && (
          <PromptCards promptType={promptType} onSendStdin={onSendStdinDirect} />
        )}
      </div>

      {/* Send Bar */}
      <form onSubmit={onSendMessage} className="p-4 border-t border-border glass-panel">
        <div className="flex flex-col gap-2.5">
          <div className="flex gap-2.5">
            <Textarea
              placeholder="发送命令或提问给智能体..."
              value={chatInput}
              onChange={(e) => setChatInput(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter" && !e.shiftKey) {
                  e.preventDefault();
                  onSendMessage(e);
                }
              }}
              className="flex-1 resize-none h-[45px] min-h-0"
            />
            <Button type="submit" className="px-5 font-semibold self-end">
              <Send className="h-4 w-4" /> 发送
            </Button>
          </div>

          <div className="flex justify-between items-center text-xs">
            <div className="flex gap-4">
              <div className="flex items-center gap-1.5">
                <span className="text-muted-foreground">工作区:</span>
                <select
                  value={chatWorkspace}
                  onChange={(e) => setChatWorkspace(e.target.value)}
                  className="bg-transparent border-none text-foreground cursor-pointer"
                >
                  <option value="direct">直聊模式</option>
                  {chatWorkspace !== "direct" && (
                    <option value={chatWorkspace}>
                      {chatWorkspace.split(/[\\/]/).pop()}
                    </option>
                  )}
                </select>
              </div>
              <div className="flex items-center gap-1.5">
                <span className="text-muted-foreground">映射模型:</span>
                <select
                  value={targetModel}
                  onChange={(e) => setTargetModel(e.target.value)}
                  className="bg-transparent border-none text-foreground cursor-pointer"
                >
                  {modelOptions.map((opt) => (
                    <option key={opt} value={opt}>{opt}</option>
                  ))}
                </select>
              </div>
            </div>

            {currentConvId && activeSessions.includes(currentConvId) && (
              <Button
                type="button"
                variant="destructive"
                size="sm"
                onClick={() => onStopSession(currentConvId)}
              >
                <Square className="h-3 w-3" /> 强行终止
              </Button>
            )}
          </div>
        </div>
      </form>
    </div>
  );
}

// ── Sub-components ──────────────────────────────────

function EmptyState({ activeAgent, onQuickPrompt }: { activeAgent: string; onQuickPrompt: (v: string) => void }) {
  return (
    <div className="flex-1 flex flex-col justify-center items-center gap-5 opacity-80">
      <div className="text-5xl">🤖</div>
      <div className="text-center">
        <h3 className="text-lg font-semibold">开始与 {activeAgent} 对话</h3>
        <p className="text-muted-foreground text-sm max-w-[450px] mt-1">
          OMNIX 已经在后台准备好了 PTY 伪终端。发送任何开发指令，智能体会在当前工作区环境下开始执行。
        </p>
      </div>
      <div className="flex flex-col gap-2.5 w-full max-w-[500px]">
        {[
          { label: "🚀 初始化一个 Vite React TS 应用", prompt: "在工作区下初始化一个 Vite React TS 应用，并配置好 ESLint 与 Prettier。" },
          { label: "🔒 诊断并修复 Rust Tokio 异步死锁", prompt: "诊断并重构项目中的异步锁设计，避免跨越 await 持有 Mutex 锁导致死锁。" },
          { label: "🧬 编写高性能并发本地缓存", prompt: "开发一个带超时淘汰与并发访问控制的高性能 Local Cache 模块。" },
        ].map((item) => (
          <Card
            key={item.label}
            className="cursor-pointer hover:bg-white/5 bg-white/[0.01]"
            onClick={() => onQuickPrompt(item.prompt)}
          >
            <CardContent className="p-3 text-left text-sm">{item.label}</CardContent>
          </Card>
        ))}
      </div>
    </div>
  );
}

function PromptCards({ promptType, onSendStdin }: { promptType: PromptType; onSendStdin: (input: string) => void }) {
  return (
    <div className="flex justify-center py-3">
      {promptType === "trust" && (
        <Card className="p-4 bg-emerald-500/[0.08] border-emerald-500/40 border-dashed max-w-[450px]">
          <h4 className="text-emerald-500 font-semibold mb-1.5">🛡️ 安全信任鉴权提示</h4>
          <p className="text-xs text-muted-foreground mb-3">
            智能体需要您授权确认是否信任并继续执行此目录下的工具和命令。
          </p>
          <div className="flex gap-2.5">
            <Button className="flex-1" size="sm" onClick={() => onSendStdin("1\n")}>
              <Shield className="h-3 w-3" /> 信任并确认
            </Button>
            <Button className="flex-1" variant="outline" size="sm" onClick={() => onSendStdin("2\n")}>
              ❌ 拒绝并退出
            </Button>
          </div>
        </Card>
      )}

      {promptType === "update" && (
        <Card className="p-4 bg-blue-500/[0.08] border-blue-500/40 border-dashed max-w-[450px]">
          <h4 className="text-blue-500 font-semibold mb-1.5">💡 智能体 CLI 存在新版本</h4>
          <p className="text-xs text-muted-foreground mb-3">
            检测到终端内抛出更新提示。是否确认现在进行自动升级？
          </p>
          <div className="flex gap-2.5">
            <Button className="flex-1" size="sm" onClick={() => onSendStdin("\r")}>确认更新</Button>
            <Button className="flex-1" variant="outline" size="sm" onClick={() => onSendStdin("\x1b")}>跳过更新</Button>
          </div>
        </Card>
      )}

      {promptType === "menu" && (
        <Card className="p-3 flex gap-2 flex-wrap justify-center bg-white/[0.02] border-dashed border-border">
          <Button variant="outline" size="sm" onClick={() => onSendStdin("\t")}>Tab 焦点</Button>
          <Button variant="outline" size="sm" onClick={() => onSendStdin(" ")}>空格 勾选</Button>
          <Button variant="outline" size="sm" onClick={() => onSendStdin("\x1b[A")}><ArrowUp className="h-3 w-3" /></Button>
          <Button variant="outline" size="sm" onClick={() => onSendStdin("\x1b[B")}><ArrowDown className="h-3 w-3" /></Button>
          <Button variant="outline" size="sm" onClick={() => onSendStdin("\x1b[D")}><ArrowLeft className="h-3 w-3" /></Button>
          <Button variant="outline" size="sm" onClick={() => onSendStdin("\x1b[C")}><ArrowRight className="h-3 w-3" /></Button>
          <Button size="sm" onClick={() => onSendStdin("\r")}><Check className="h-3 w-3" /> 确认</Button>
        </Card>
      )}

      {promptType === "editor" && (
        <div className="flex justify-center">
          <Button size="sm" onClick={() => onSendStdin("\x1b[Z")}>
            📤 提交/发送 (Shift+Tab)
          </Button>
        </div>
      )}
    </div>
  );
}

// ── Helpers ─────────────────────────────────────────

function buildModelOptions(activeModels: PlatformModel[], targetModel: string): string[] {
  const list = [...activeModels.map((m) => m.model_name)];
  DEFAULT_MODEL_NAMES.forEach((d) => {
    if (!list.includes(d)) list.push(d);
  });
  if (targetModel && !list.includes(targetModel)) {
    list.push(targetModel);
  }
  return list;
}

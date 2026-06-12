/**
 * TeamTab — 多智能体多窗口团队协同
 *
 * Left: PTY interactive pane, Right: PlanTree
 */

import { useEffect, useRef } from "react";
import { Button } from "@/components/ui/button";
import { Textarea } from "@/components/ui/textarea";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Play, Square, Send } from "lucide-react";
import { PlanTree } from "@/PlanTree";
import type { ConversationInfo, DetectedAgent } from "@/types";

interface TeamTabProps {
  currentConvId: string;
  conversations: ConversationInfo[];
  activeAgent: string;
  detectedAgents: DetectedAgent[];
  activeSessions: string[];
  collabLogs: string;
  collabStdin: string;
  rightPaneWidth: number;
  onSelectConversation: (id: string) => void;
  setActiveAgent: (name: string) => void;
  setCollabStdin: (val: string) => void;
  onStartSession: () => void;
  onStopSession: (id: string) => void;
  onSendStdinDirect: (input: string) => void;
  startResizing: (e: React.MouseEvent) => void;
}

export function TeamTab({
  currentConvId,
  conversations,
  activeAgent,
  detectedAgents,
  activeSessions,
  collabLogs,
  collabStdin,
  rightPaneWidth,
  onSelectConversation,
  setActiveAgent,
  setCollabStdin,
  onStartSession,
  onStopSession,
  onSendStdinDirect,
  startResizing,
}: TeamTabProps) {
  const isSessionRunning = currentConvId && activeSessions.includes(currentConvId);

  // Auto-resize Stdin textarea
  const stdinRef = useRef<HTMLTextAreaElement>(null);
  useEffect(() => {
    const el = stdinRef.current;
    if (!el) return;
    el.style.height = "auto";
    const next = Math.min(Math.max(el.scrollHeight, 48), 240);
    el.style.height = `${next}px`;
  }, [collabStdin]);

  const handleSend = () => {
    if (collabStdin.trim()) {
      onSendStdinDirect(collabStdin + "\n");
      setCollabStdin("");
    }
  };

  return (
    <div className="flex h-full overflow-hidden flex-1">
      {/* Left PTY Pane */}
      <div className="flex flex-col h-full p-4 gap-3 flex-1">
        {/* Controls */}
        <div className="flex gap-2.5 items-center">
          <div className="flex-1">
            <label className="text-xs text-muted-foreground block mb-1">活动会话</label>
            <Select value={currentConvId} onValueChange={onSelectConversation}>
              <SelectTrigger>
                <SelectValue placeholder="-- 请选择会话 --" />
              </SelectTrigger>
              <SelectContent>
                {conversations.map((c) => (
                  <SelectItem key={c.id} value={c.id}>{c.title}</SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
          <div>
            <label className="text-xs text-muted-foreground block mb-1">执行 Agent</label>
            <Select value={activeAgent} onValueChange={setActiveAgent}>
              <SelectTrigger>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {detectedAgents.map((a) => (
                  <SelectItem key={a.name} value={a.name}>{a.name}</SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>

          <div className="flex self-end gap-1.5">
            <Button
              size="sm"
              onClick={onStartSession}
              disabled={!currentConvId || !!isSessionRunning}
            >
              <Play className="h-3 w-3" /> 启动
            </Button>
            <Button
              size="sm"
              variant="outline"
              onClick={() => currentConvId && onStopSession(currentConvId)}
              disabled={!currentConvId || !isSessionRunning}
            >
              <Square className="h-3 w-3 text-destructive" /> 停止
            </Button>
          </div>
        </div>

        {/* Log Display */}
        <div className="flex-1 min-h-[150px] bg-[#050508] border border-border rounded-lg p-3 flex flex-col overflow-hidden">
          <div className="flex-1 overflow-y-auto font-mono text-sm text-lime-400 whitespace-pre-wrap">
            {collabLogs || "等待智能体启动并输出日志..."}
          </div>
        </div>

        {/* Stdin Input (auto-grows; Enter = send, Shift+Enter = newline) */}
        <div className="flex gap-2.5 items-end">
          <Textarea
            ref={stdinRef}
            placeholder="输入标准输入指令... (Enter 发送，Shift+Enter 换行)"
            value={collabStdin}
            onChange={(e) => setCollabStdin(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && !e.shiftKey) {
                e.preventDefault();
                handleSend();
              }
            }}
            className="flex-1 resize-none min-h-[48px] overflow-y-auto"
            style={{ maxHeight: "240px" }}
          />
          <Button onClick={handleSend} className="self-end">
            <Send className="h-4 w-4" /> 发送
          </Button>
        </div>
      </div>

      {/* Resize Handle */}
      <div
        className="w-2 bg-muted/20 cursor-col-resize h-full hover:bg-muted/30 transition-colors"
        onMouseDown={startResizing}
      />

      {/* Right PlanTree */}
      <div
        className="flex flex-col border-l border-border"
        style={{ width: `${rightPaneWidth}px` }} // Dynamic width — cannot use Tailwind for runtime px values
      >
        <div className="px-4 py-3 border-b border-border bg-muted/5">
          <h3 className="text-sm font-semibold m-0">👥 协同任务计划树</h3>
        </div>
        <div className="flex-1 overflow-y-auto">
          {currentConvId ? (
            <PlanTree conversationId={currentConvId} containerWidth={rightPaneWidth} />
          ) : (
            <div className="p-5 text-center text-muted-foreground text-sm">
              请先在左侧选择一个活动会话
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

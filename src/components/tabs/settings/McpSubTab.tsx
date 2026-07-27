/** Split from SettingsTab.tsx — pure move, no behavior change. */
import { useEffect, useState } from "react";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { Badge } from "@/components/ui/badge";
import { Edit, ExternalLink, Key, Plug, Plus, Save, Store, Trash2 } from "lucide-react";
import { cn } from "@/lib/utils";
import { toast } from "@/components/ui/sonner";
import { mcpSyncApi } from "@/lib/tauri-api";
import type { AgentMcpState } from "@/lib/tauri-api";
import { openUrl } from "@tauri-apps/plugin-opener";
import type { McpServer } from "@/types";
import type { SettingsTabProps } from "./types";

export function McpSubTab({
  mcpServers,
  onOpenMcpModal,
  onDeleteMcpServer,
  showMcpModal,
  editingMcpServer,
  mcpForm,
  onCloseMcpModal,
  onUpdateMcpForm,
  onSaveMcpServer,
  onReloadMcpServers,
}: SettingsTabProps) {
  const [agentStates, setAgentStates] = useState<AgentMcpState[]>([]);
  const [syncBusy, setSyncBusy] = useState("");
  const [importBusy, setImportBusy] = useState("");
  const loadAgentStates = () => mcpSyncApi.getAgentStates().then(setAgentStates).catch(() => {});
  useEffect(() => { loadAgentStates(); }, []);

  // Sync targets: OMNIX-managed MCP → each agent's native config.
  const SYNC_TARGETS = ["claude_code", "codex", "gemini", "opencode"];
  const AGENT_LABELS: Record<string, string> = {
    claude_code: "Claude", codex: "Codex", gemini: "Gemini", opencode: "OpenCode",
  };
  const agentLabel = (agent: string) => AGENT_LABELS[agent] ?? agent;
  const isSynced = (serverName: string, agent: string) =>
    agentStates.find((state) => state.agent === agent)?.server_names.includes(serverName) ?? false;

  const syncServer = async (server: McpServer) => {
    setSyncBusy(server.id);
    try {
      const reports = await mcpSyncApi.syncToAgents(SYNC_TARGETS, [server.id]);
      await loadAgentStates();
      const skipped = reports.flatMap((report) => report.skipped);
      toast.success(`已同步「${server.name}」到各 Agent${skipped.length ? `（部分跳过：${skipped.join("；")}）` : ""}`);
    } catch (error) {
      toast.error(`同步失败：${error}`);
    } finally {
      setSyncBusy("");
    }
  };

  // Reverse import: pull an agent's native MCP servers into OMNIX.
  const importFromAgent = async (agent: string) => {
    setImportBusy(agent);
    try {
      const names = await mcpSyncApi.importFromAgent(agent);
      await Promise.all([loadAgentStates(), onReloadMcpServers?.()]);
      toast.success(names.length ? `已从 ${agentLabel(agent)} 导入 ${names.length} 个 MCP：${names.join("、")}` : `${agentLabel(agent)} 没有可导入的 MCP`);
    } catch (error) {
      toast.error(`导入失败：${error}`);
    } finally {
      setImportBusy("");
    }
  };

  const unsyncServer = async (server: McpServer, agent: string) => {
    setSyncBusy(server.id);
    try {
      await mcpSyncApi.removeFromAgent(agent, server.name);
      await loadAgentStates();
      toast.success(`已从 ${agentLabel(agent)} 撤销「${server.name}」`);
    } catch (error) {
      toast.error(`撤销失败：${error}`);
    } finally {
      setSyncBusy("");
    }
  };

  return (
    <div className="flex flex-col gap-4 max-w-4xl mx-auto">
      <Card>
        <CardHeader className="flex-row justify-between items-center mb-4">
          <div>
            <CardTitle className="text-sm">🔌 MCP 服务器管理</CardTitle>
            <p className="mt-1 text-xs text-muted-foreground">配一次，点「同步」即可写入 Claude Code / Codex / Gemini / OpenCode 的原生配置（写前自动备份，可单独撤销）。</p>
          </div>
          <Button size="sm" variant="outline" onClick={() => onOpenMcpModal()}>
            <Plus className="h-3 w-3" /> 新增
          </Button>
        </CardHeader>
        <CardContent>
          {/* Reverse import: pull each agent's native MCP servers into OMNIX. */}
          <div className="mb-3 flex flex-wrap items-center gap-2 rounded-md border border-dashed border-border px-3 py-2">
            <span className="text-xs text-muted-foreground">从 Agent 导入现有 MCP：</span>
            {SYNC_TARGETS.map((agent) => (
              <button
                key={agent}
                disabled={importBusy === agent}
                onClick={() => importFromAgent(agent)}
                className="rounded border border-border px-2 py-0.5 text-xs text-muted-foreground hover:bg-muted/30 hover:text-foreground disabled:opacity-50"
              >
                {importBusy === agent ? "导入中…" : agentLabel(agent)}
              </button>
            ))}
          </div>
          {mcpServers.length === 0 ? (
            <div className="py-2.5 text-center text-muted-foreground text-xs">
              暂无 MCP 服务器配置。点击"新增"添加。
            </div>
          ) : (
            <div className="flex flex-col gap-2.5">
              {mcpServers.map((srv) => (
                <div
                  key={srv.id}
                  className="flex justify-between items-center p-2.5 border-b border-border rounded-md"
                >
                  <div className="flex items-center gap-2">
                    <Badge variant={srv.server_type === "stdio" ? "default" : "secondary"}>
                      {srv.server_type.toUpperCase()}
                    </Badge>
                    <span className="font-semibold text-sm">{srv.name}</span>
                    {srv.server_type === "stdio" && (
                      <span className="text-xs text-muted-foreground">
                        <code>{srv.command}</code>
                      </span>
                    )}
                    {srv.server_type === "sse" && (
                      <span className="text-xs text-muted-foreground">
                        <code>{srv.url}</code>
                      </span>
                    )}
                  </div>
                  <div className="flex items-center gap-1.5">
                    {SYNC_TARGETS.map((agent) => {
                      const synced = isSynced(srv.name, agent);
                      return (
                        <button
                          key={agent}
                          onClick={() => synced && unsyncServer(srv, agent)}
                          disabled={syncBusy === srv.id || !synced}
                          title={synced ? `已同步到 ${agentLabel(agent)}（点击撤销）` : `未同步到 ${agentLabel(agent)}`}
                          className={cn(
                            "rounded border px-1.5 py-0.5 text-[10px]",
                            synced ? "border-success/40 text-success" : "border-border text-muted-foreground opacity-60",
                            synced && "cursor-pointer hover:bg-success/10"
                          )}
                        >
                          {agentLabel(agent)} {synced ? "✓" : "—"}
                        </button>
                      );
                    })}
                    <Button size="sm" variant="outline" disabled={syncBusy === srv.id} onClick={() => syncServer(srv)} title="同步到 Claude Code / Codex / Gemini / OpenCode">
                      <Plug className="h-3 w-3" /> 同步
                    </Button>
                    <Badge variant={srv.is_enabled ? "default" : "secondary"}>
                      {srv.is_enabled ? "启用" : "禁用"}
                    </Badge>
                    <Button size="sm" variant="outline" onClick={() => onOpenMcpModal(srv)}>
                      <Edit className="h-3 w-3" />
                    </Button>
                    <Button size="sm" variant="outline" onClick={() => onDeleteMcpServer(srv.id)}>
                      <Trash2 className="h-3 w-3 text-destructive" />
                    </Button>
                  </div>
                </div>
              ))}
            </div>
          )}
        </CardContent>
      </Card>

      {/* ── MCP Market (Discover) ──────────────────────── */}
      <Card>
        <CardHeader>
          <CardTitle className="text-sm flex items-center gap-2">
            <Store className="h-4 w-4" /> 🛒 发现 MCP 服务器
          </CardTitle>
        </CardHeader>
        <CardContent className="flex flex-col gap-3">
          <span className="text-xs text-muted-foreground">
            浏览 MCP 市场以发现和安装新的 MCP 服务器，扩展 AI 的工具能力。
          </span>
          <div className="grid grid-cols-1 sm:grid-cols-2 gap-2">
            {([
              { name: "Smithery", desc: "MCP 服务器市场与注册中心", url: "https://smithery.ai", color: "text-blue-400" },
              { name: "MCP.so", desc: "MCP 服务器目录与搜索", url: "https://mcp.so", color: "text-emerald-400" },
              { name: "Glama", desc: "MCP 服务器与 AI 工具发现", url: "https://glama.ai/mcp/servers", color: "text-purple-400" },
              { name: "MCP Hub", desc: "官方 MCP 服务器集合", url: "https://github.com/modelcontextprotocol/servers", color: "text-orange-400" },
            ]).map(market => (
              <button
                key={market.name}
                className="flex items-center gap-2.5 p-3 rounded-lg border border-border/50 bg-background/30 hover:bg-background/60 transition-all text-left cursor-pointer"
                onClick={() => {
                  openUrl(market.url).catch(() => window.open(market.url, "_blank"));
                }}
              >
                <ExternalLink className={cn("h-4 w-4 shrink-0", market.color)} />
                <div className="min-w-0">
                  <span className="text-sm font-medium block">{market.name}</span>
                  <span className="text-xs text-muted-foreground">{market.desc}</span>
                </div>
              </button>
            ))}
          </div>
        </CardContent>
      </Card>

      {/* ── MCP Providers ──────────────────────────────── */}
      <Card>
        <CardHeader>
          <CardTitle className="text-sm flex items-center gap-2">
            <Key className="h-4 w-4" /> 🔑 MCP 供应商
          </CardTitle>
        </CardHeader>
        <CardContent className="flex flex-col gap-3">
          <span className="text-xs text-muted-foreground">
            通过 API Token 从 MCP 供应商批量获取和安装 MCP 服务器。
          </span>
          <div className="flex flex-col gap-2.5">
            {([
              { name: "通义百炼", desc: "阿里云百炼平台 MCP 服务", url: "https://bailian.console.aliyun.com/" },
              { name: "ModelScope", desc: "魔搭社区 MCP 工具", url: "https://modelscope.cn/" },
              { name: "蓝耘", desc: "蓝耘科技 MCP 服务", url: "https://cloud.lanyun.net/" },
              { name: "302.AI", desc: "302.AI MCP 网关", url: "https://302.ai/" },
              { name: "MCP Router", desc: "MCP 路由代理服务", url: "https://mcprouter.com/" },
            ]).map(provider => (
              <div key={provider.name} className="flex justify-between items-center p-2.5 border border-border/30 rounded-md bg-background/20">
                <div className="min-w-0">
                  <span className="text-sm font-medium">{provider.name}</span>
                  <span className="text-xs text-muted-foreground ml-2">{provider.desc}</span>
                </div>
                <Button
                  size="sm"
                  variant="outline"
                  className="text-xs gap-1"
                  onClick={() => {
                    openUrl(provider.url).catch(() => window.open(provider.url, "_blank"));
                  }}
                >
                  <ExternalLink className="h-3 w-3" /> 官网
                </Button>
              </div>
            ))}
          </div>
        </CardContent>
      </Card>

      {/* MCP Server Modal */}
      {showMcpModal && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
          <div className="glass-card p-6 w-full max-w-[480px] mx-4 max-h-[80vh] overflow-y-auto">
            <h3 className="text-lg font-semibold mb-4">
              {editingMcpServer ? "编辑 MCP 服务器" : "新增 MCP 服务器"}
            </h3>
            <div className="flex flex-col gap-3">
              <div className="space-y-1.5">
                <Label>名称</Label>
                <Input value={mcpForm.name} onChange={(e) => onUpdateMcpForm("name", e.target.value)} />
              </div>
              <div className="space-y-1.5">
                <Label>类型</Label>
                <div className="flex gap-2">
                  {(["stdio", "sse"] as const).map((t) => (
                    <button
                      key={t}
                      onClick={() => onUpdateMcpForm("server_type", t)}
                      className={cn(
                        "flex-1 p-2 rounded-md border text-sm",
                        mcpForm.server_type === t
                          ? "border-primary bg-primary/10"
                          : "border-border"
                      )}
                    >
                      {t === "stdio" ? "STDIO (命令行)" : "SSE (HTTP)"}
                    </button>
                  ))}
                </div>
              </div>
              {mcpForm.server_type === "stdio" && (
                <>
                  <div className="space-y-1.5">
                    <Label>启动命令</Label>
                    <Input value={mcpForm.command} onChange={(e) => onUpdateMcpForm("command", e.target.value)} placeholder="npx" />
                  </div>
                  <div className="space-y-1.5">
                    <Label>参数 (JSON 数组)</Label>
                    <Input value={mcpForm.args} onChange={(e) => onUpdateMcpForm("args", e.target.value)} placeholder='["-y", "@modelcontextprotocol/server"]' />
                  </div>
                  <div className="space-y-1.5">
                    <Label>环境变量 (JSON 对象)</Label>
                    <Input value={mcpForm.env} onChange={(e) => onUpdateMcpForm("env", e.target.value)} placeholder='{"KEY": "value"}' />
                  </div>
                </>
              )}
              {mcpForm.server_type === "sse" && (
                <div className="space-y-1.5">
                  <Label>服务器 URL</Label>
                  <Input value={mcpForm.url} onChange={(e) => onUpdateMcpForm("url", e.target.value)} placeholder="http://localhost:3001/sse" />
                </div>
              )}
              <div className="flex items-center gap-2">
                <Switch checked={mcpForm.is_enabled} onCheckedChange={(v) => onUpdateMcpForm("is_enabled", v)} id="mcp_enabled" />
                <Label htmlFor="mcp_enabled">启用</Label>
              </div>
            </div>
            <div className="flex justify-end gap-2 mt-4">
              <Button variant="ghost" onClick={onCloseMcpModal}>取消</Button>
              <Button onClick={async () => {
                try {
                  await onSaveMcpServer();
                  toast.success("MCP 服务器保存成功！");
                } catch (e) {
                  toast.error("保存失败：" + String(e));
                }
              }}>
                <Save className="h-4 w-4" /> 保存
              </Button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

// ── Backup Sub-Tab ──────────────────────────────────────


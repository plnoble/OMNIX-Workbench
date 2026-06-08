import { useState } from "react";

/**
 * SettingsTab — 模型中转代理与网关设置
 *
 * Sub-tabs: Platform (Model Hub), System, MCP Servers, Backup
 */

import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { Checkbox } from "@/components/ui/checkbox";
import { Separator } from "@/components/ui/separator";
import { Badge } from "@/components/ui/badge";
import {
  Plus, Edit, Trash2, RefreshCw, Eye, Brain, Mic, Code,
  Maximize2, Wrench, Layers, Zap, Activity,
  Save, Plug, Settings, MousePointerClick, Clock, Copy,
  Languages, ArrowRightLeft, Search, Server, Database, Download, Upload,
} from "lucide-react";
import { cn } from "@/lib/utils";
import { toast } from "@/components/ui/sonner";
import { BUILTIN_LANGUAGES } from "@/lib/translate-constants";
import type { ModelPlatform, PlatformModel, AgentAccount, ModelTestState, SettingsSubTab, SelectionHistoryEntry, SearchProvider, WebSearchResult, McpServer, BackupTableInfo, ImportResult } from "@/types";

interface SettingsTabProps {
  settingsSubTab: SettingsSubTab;
  setSettingsSubTab: (tab: SettingsSubTab) => void;

  // Platform sub-tab
  platforms: ModelPlatform[];
  selectedPlatformId: string;
  platformModels: PlatformModel[];
  modelTestingState: Record<string, ModelTestState>;
  fetchingModels: boolean;
  onSelectPlatform: (id: string) => void;
  onTogglePlatform: (plat: ModelPlatform) => void;
  onAddPlatform: () => void;
  onEditPlatform: (plat: ModelPlatform) => void;
  onDeletePlatform: (id: string) => void;
  onFetchRemoteModels: () => void;
  onAddModel: () => void;
  onToggleModelEnabled: (model: PlatformModel) => void;
  onToggleCapability: (model: PlatformModel, field: "vision" | "audio" | "reasoning" | "coding" | "long_context" | "tool_use" | "embedding" | "speedy") => void;
  onTestModel: (id: string) => Promise<string>;
  onDeleteModel: (id: string) => void;
  batchTesting: Record<string, boolean>;
  onBatchTestModels: (platformId: string) => void;

  // System sub-tab
  accounts: AgentAccount[];
  onAddAccount: () => void;
  onEditAccount: (acc: AgentAccount) => void;
  onDeleteAccount: (id: string) => void;
  onSwitchAccount: (id: string) => void;

  // Settings form
  apiKey: string;
  apiHost: string;
  targetModel: string;
  proxyPort: string;
  gpuAcceleration: boolean;
  idleTimeout: string;
  autoStart: boolean;
  startToTray: boolean;
  useWsl: boolean;
  wslDistro: string;
  setApiKey: (v: string) => void;
  setApiHost: (v: string) => void;
  setTargetModel: (v: string) => void;
  setProxyPort: (v: string) => void;
  setGpuAcceleration: (v: boolean) => void;
  setIdleTimeout: (v: string) => void;
  setAutoStart: (v: boolean) => void;
  setStartToTray: (v: boolean) => void;
  setUseWsl: (v: boolean) => void;
  setWslDistro: (v: string) => void;
  onSaveSettings: () => Promise<void>;

  // Selection Assistant
  selectionShortcut: string;
  selectionCaptureMode: string;
  selectionShowOnCapture: boolean;
  selectionPreserveClipboard: boolean;
  isSelectionCapturing: boolean;
  lastSelectionCapture: string | null;
  selectionCaptureError: string | null;
  selectionHistory: SelectionHistoryEntry[];
  onSetSelectionShortcut: (v: string) => void;
  onSetSelectionCaptureMode: (v: string) => void;
  onSetSelectionShowOnCapture: (v: boolean) => void;
  onSetSelectionPreserveClipboard: (v: boolean) => void;
  onTestSelectionCapture: () => Promise<string | null>;
  onSaveSelectionSettings: (updates: Record<string, unknown>) => Promise<void>;
  onLoadSelectionHistory: () => Promise<void>;
  onDeleteSelectionHistoryItem: (id: string) => Promise<void>;
  onClearSelectionHistory: () => Promise<void>;

  // Translation
  translatePreferredLang: string;
  translateAlterLang: string;
  translateModel: string;
  translateAutoDetect: boolean;
  translateCustomPrompt: string;
  onSetTranslatePreferredLang: (v: string) => void;
  onSetTranslateAlterLang: (v: string) => void;
  onSetTranslateModel: (v: string) => void;
  onSetTranslateAutoDetect: (v: boolean) => void;
  onSetTranslateCustomPrompt: (v: string) => void;
  onSaveTranslationSettings: (updates: Record<string, unknown>) => Promise<void>;

  // Theme
  themeMode: "dark" | "light" | "auto";
  onSetThemeMode: (v: "dark" | "light" | "auto") => void;

  // Search
  searchProviders: SearchProvider[];
  searchSelectedProviderId: string;
  searchResults: WebSearchResult[];
  searchQuery: string;
  isSearching: boolean;
  onSetSearchQuery: (q: string) => void;
  onSetSearchSelectedProviderId: (id: string) => void;
  onSearch: (query: string) => Promise<WebSearchResult[]>;
  onAddSearchProvider: () => void;
  onEditSearchProvider: (provider: SearchProvider) => void;
  onDeleteSearchProvider: (id: string) => Promise<void>;

  // MCP Servers
  mcpServers: McpServer[];
  showMcpModal: boolean;
  editingMcpServer: McpServer | null;
  mcpForm: { id: string; name: string; command: string; args: string; env: string; url: string; server_type: "stdio" | "sse"; is_enabled: boolean };
  onOpenMcpModal: (server?: McpServer) => void;
  onCloseMcpModal: () => void;
  onUpdateMcpForm: (field: string, value: string | boolean) => void;
  onSaveMcpServer: () => Promise<void>;
  onDeleteMcpServer: (id: string) => Promise<void>;

  // Backup
  backupTableInfo: BackupTableInfo[];
  backupSelectedTables: Set<string>;
  isBackupExporting: boolean;
  isBackupImporting: boolean;
  lastImportResult: ImportResult | null;
  onLoadBackupInfo: () => Promise<void>;
  onToggleBackupTable: (tableName: string) => void;
  onSelectAllBackupTables: () => void;
  onDeselectAllBackupTables: () => void;
  onExportBackup: () => Promise<string | null>;
  onImportBackup: (jsonStr: string) => Promise<ImportResult | null>;
}

export function SettingsTab(props: SettingsTabProps) {
  return (
    <div className="flex h-full overflow-hidden flex-1">
      {/* Left sub-tab navigation */}
      <div className="w-40 border-r border-border p-4 flex flex-col gap-2 bg-[rgba(10,10,14,0.1)]">
        <Button
          variant={props.settingsSubTab === "platform" ? "default" : "ghost"}
          className="w-full justify-start"
          onClick={() => props.setSettingsSubTab("platform")}
        >
          <Plug className="h-4 w-4" /> 大模型平台
        </Button>
        <Button
          variant={props.settingsSubTab === "system" ? "default" : "ghost"}
          className="w-full justify-start"
          onClick={() => props.setSettingsSubTab("system")}
        >
          <Settings className="h-4 w-4" /> 系统设置
        </Button>
        <Button
          variant={props.settingsSubTab === "mcp" ? "default" : "ghost"}
          className="w-full justify-start"
          onClick={() => props.setSettingsSubTab("mcp")}
        >
          <Server className="h-4 w-4" /> MCP 服务器
        </Button>
        <Button
          variant={props.settingsSubTab === "backup" ? "default" : "ghost"}
          className="w-full justify-start"
          onClick={() => props.setSettingsSubTab("backup")}
        >
          <Database className="h-4 w-4" /> 数据备份
        </Button>
      </div>

      {/* Right panel */}
      <div className="flex-1 overflow-y-auto p-5">
        {props.settingsSubTab === "platform" && <PlatformSubTab {...props} />}
        {props.settingsSubTab === "system" && <SystemSubTab {...props} />}
        {props.settingsSubTab === "mcp" && <McpSubTab {...props} />}
        {props.settingsSubTab === "backup" && <BackupSubTab {...props} />}
      </div>
    </div>
  );
}

// ── Platform Sub-Tab ─────────────────────────────────

function PlatformSubTab({
  platforms,
  selectedPlatformId,
  platformModels,
  modelTestingState,
  fetchingModels,
  onSelectPlatform,
  onTogglePlatform,
  onAddPlatform,
  onEditPlatform,
  onDeletePlatform,
  onFetchRemoteModels,
  onAddModel,
  onToggleModelEnabled,
  onToggleCapability,
  onTestModel,
  onDeleteModel,
  batchTesting,
  onBatchTestModels,
}: SettingsTabProps) {
  const selectedPlatform = platforms.find((p) => p.id === selectedPlatformId);

  return (
    <div className="flex h-full gap-5">
      {/* Setup guide banner when no platform has API key */}
      {platforms.every(p => !p.api_key) && (
        <div className="absolute top-2 left-64 right-2 z-10 bg-amber-500/10 border border-amber-500/30 rounded-lg px-4 py-2.5 text-xs text-amber-400 flex items-center gap-2">
          <Zap className="h-4 w-4 flex-shrink-0" />
          <span>
            <strong>快速开始：</strong>在下方选择一个平台，填入 API Key 后启用模型，即可使用 QA 翻译、知识库等 AI 功能。
            推荐先配置 <strong>DeepSeek</strong>（国内直连）或 <strong>Ollama</strong>（本地免费）。
          </span>
        </div>
      )}

      {/* Platform List Sidebar */}
      <div className="w-64 border-r border-border pr-5 flex flex-col gap-3">
        <div className="flex justify-between items-center">
          <span className="text-sm font-semibold text-muted-foreground">模型提供商平台</span>
          <Button size="sm" variant="outline" onClick={onAddPlatform}>
            <Plus className="h-3 w-3" />
          </Button>
        </div>

        <div className="flex flex-col gap-2 flex-1 overflow-y-auto">
          {platforms.length === 0 ? (
            <div className="py-5 text-center text-muted-foreground text-xs">无平台</div>
          ) : (
            platforms.map((plat) => {
              const isActive = selectedPlatformId === plat.id;
              return (
                <div
                  key={plat.id}
                  className={cn(
                    "p-2.5 rounded-lg border cursor-pointer flex justify-between items-center transition-all",
                    isActive ? "bg-accent/[0.06] border-accent/30" : "bg-white/[0.01] border-border hover:bg-white/5"
                  )}
                  onClick={() => onSelectPlatform(plat.id)}
                >
                  <div>
                    <span className="font-semibold text-sm block">{plat.name}</span>
                    <span className="text-[10px] text-muted-foreground">类型: {plat.api_type}</span>
                  </div>
                  <div onClick={(e) => e.stopPropagation()}>
                    <Switch
                      checked={plat.is_enabled}
                      onCheckedChange={() => onTogglePlatform(plat)}
                    />
                  </div>
                </div>
              );
            })
          )}
        </div>
      </div>

      {/* Platform Detail */}
      <div className="flex-1 flex flex-col gap-4">
        {selectedPlatform ? (
          <>
            {/* Header */}
            <Card>
              <CardContent className="p-4 flex justify-between items-center">
                <div>
                  <h3 className="text-base font-semibold mb-1">{selectedPlatform.name}</h3>
                  <span className="text-xs text-muted-foreground">
                    Endpoint: <code>{selectedPlatform.api_address}</code>
                  </span>
                </div>
                <div className="flex gap-2">
                  <Button size="sm" variant="outline" onClick={onFetchRemoteModels} disabled={fetchingModels}>
                    <RefreshCw className={cn("h-3 w-3", fetchingModels && "animate-spin")} />
                    {fetchingModels ? "拉取中..." : "获取模型列表"}
                  </Button>
                  <Button size="sm" variant="outline" onClick={() => onBatchTestModels(selectedPlatform.id)} disabled={batchTesting[selectedPlatform.id]}>
                    {batchTesting[selectedPlatform.id] ? <RefreshCw className="h-3 w-3 animate-spin" /> : <Activity className="h-3 w-3" />}
                    {batchTesting[selectedPlatform.id] ? "检测中..." : "健康检测"}
                  </Button>
                  <Button size="sm" variant="outline" onClick={() => onEditPlatform(selectedPlatform)}>
                    <Edit className="h-3 w-3" /> 编辑
                  </Button>
                  <Button size="sm" variant="outline" onClick={() => onDeletePlatform(selectedPlatform.id)}>
                    <Trash2 className="h-3 w-3 text-destructive" /> 删除
                  </Button>
                </div>
              </CardContent>
            </Card>

            {/* Models List */}
            <Card className="flex-1 flex flex-col overflow-hidden">
              <div className="flex justify-between items-center mb-3">
                <span className="text-sm font-semibold">模型列表</span>
                <Button size="sm" variant="outline" onClick={onAddModel}>
                  <Plus className="h-3 w-3" /> 自定义模型
                </Button>
              </div>

              <div className="flex-1 overflow-y-auto flex flex-col gap-2">
                {platformModels.length === 0 ? (
                  <div className="text-center text-muted-foreground py-10 text-xs">
                    暂无可用模型，请点击上方"一键拉取模型"自动从服务商同步。
                  </div>
                ) : (
                  platformModels.map((model) => {
                    const testState = modelTestingState[model.id] || "idle";
                    return (
                      <div
                        key={model.id}
                        className="flex justify-between items-center px-3 py-2 border-b border-white/[0.02]"
                      >
                        <div className="flex items-center gap-2.5">
                          <Checkbox
                            checked={model.is_enabled}
                            onCheckedChange={() => onToggleModelEnabled(model)}
                          />
                          <span className={cn("text-sm font-medium", !model.is_enabled && "opacity-60")}>
                            {model.model_name}
                          </span>
                        </div>

                        <div className="flex items-center gap-4">
                          {/* 9-Dimension Capability Icons */}
                          <div className="flex gap-0.5">
                            {([
                              { key: "has_vision" as keyof PlatformModel, icon: <Eye className="h-3 w-3" />, title: "视觉", color: "text-blue-400", field: "vision" as const },
                              { key: "has_audio" as keyof PlatformModel, icon: <Mic className="h-3 w-3" />, title: "音频", color: "text-purple-400", field: "audio" as const },
                              { key: "has_reasoning" as keyof PlatformModel, icon: <Brain className="h-3 w-3" />, title: "推理", color: "text-amber-400", field: "reasoning" as const },
                              { key: "has_coding" as keyof PlatformModel, icon: <Code className="h-3 w-3" />, title: "编程", color: "text-green-400", field: "coding" as const },
                              { key: "has_long_context" as keyof PlatformModel, icon: <Maximize2 className="h-3 w-3" />, title: "长上下文", color: "text-cyan-400", field: "long_context" as const },
                              { key: "has_tool_use" as keyof PlatformModel, icon: <Wrench className="h-3 w-3" />, title: "工具调用", color: "text-orange-400", field: "tool_use" as const },
                              { key: "has_embedding" as keyof PlatformModel, icon: <Layers className="h-3 w-3" />, title: "嵌入", color: "text-pink-400", field: "embedding" as const },
                              { key: "has_speedy" as keyof PlatformModel, icon: <Zap className="h-3 w-3" />, title: "快速", color: "text-yellow-400", field: "speedy" as const },
                            ]).map(({ key, icon, title, color, field }) => {
                              const isActive = model[key] as boolean;
                              return (
                                <button
                                  key={key}
                                  title={title}
                                  onClick={() => onToggleCapability(model, field)}
                                  className={cn(
                                    "border-none bg-transparent cursor-pointer p-0.5",
                                    isActive ? `opacity-100 ${color}` : "opacity-30 text-muted-foreground"
                                  )}
                                >
                                  {icon}
                                </button>
                              );
                            })}
                          </div>

                          {/* Test Status */}
                          <div className="flex items-center gap-1.5">
                            <div
                              className={cn(
                                "w-2 h-2 rounded-full",
                                testState === "success" && "bg-emerald-500 shadow-[0_0_8px_#10b981]",
                                testState === "error" && "bg-red-500 shadow-[0_0_8px_#ef4444]",
                                testState === "testing" && "bg-amber-500",
                                testState === "idle" && "bg-gray-500"
                              )}
                            />
                            <Button
                              size="sm"
                              variant="outline"
                              onClick={() => onTestModel(model.id)}
                              disabled={testState === "testing"}
                              className="text-[10px] px-2 py-0.5"
                            >
                              {testState === "testing" ? "测试中..." : "⚡ 测试"}
                            </Button>
                          </div>

                          <button
                            onClick={() => onDeleteModel(model.id)}
                            className="bg-transparent border-none text-destructive cursor-pointer"
                          >
                            <Trash2 className="h-3 w-3" />
                          </button>
                        </div>
                      </div>
                    );
                  })
                )}
              </div>
            </Card>
          </>
        ) : (
          <div className="flex flex-1 justify-center items-center text-muted-foreground text-sm">
            请在左侧选择一个模型平台配置以查看详情
          </div>
        )}
      </div>
    </div>
  );
}

// ── System Sub-Tab ───────────────────────────────────

function SystemSubTab({
  accounts,
  onAddAccount,
  onEditAccount,
  onDeleteAccount,
  onSwitchAccount,
  apiKey, setApiKey,
  apiHost, setApiHost,
  targetModel, setTargetModel,
  proxyPort, setProxyPort,
  gpuAcceleration, setGpuAcceleration,
  idleTimeout, setIdleTimeout,
  autoStart, setAutoStart,
  startToTray, setStartToTray,
  useWsl, setUseWsl,
  wslDistro, setWslDistro,
  onSaveSettings,
  selectionShortcut, onSetSelectionShortcut,
  selectionCaptureMode, onSetSelectionCaptureMode,
  selectionShowOnCapture, onSetSelectionShowOnCapture,
  selectionPreserveClipboard, onSetSelectionPreserveClipboard,
  isSelectionCapturing,
  lastSelectionCapture: _lastSelectionCapture,
  selectionCaptureError,
  selectionHistory,
  onTestSelectionCapture,
  onSaveSelectionSettings,
  onLoadSelectionHistory,
  onDeleteSelectionHistoryItem,
  onClearSelectionHistory,
  translatePreferredLang, onSetTranslatePreferredLang,
  translateAlterLang, onSetTranslateAlterLang,
  translateModel, onSetTranslateModel,
  translateAutoDetect, onSetTranslateAutoDetect,
  translateCustomPrompt, onSetTranslateCustomPrompt,
  onSaveTranslationSettings,
  themeMode,
  onSetThemeMode,
  searchProviders,
  onEditSearchProvider,
  onDeleteSearchProvider,
}: SettingsTabProps) {
  return (
    <div className="flex flex-col gap-4 max-w-[650px]">
      {/* Theme Selector */}
      <Card>
        <CardHeader>
          <CardTitle className="text-sm flex items-center gap-2">
            🎨 外观主题
          </CardTitle>
        </CardHeader>
        <CardContent>
          <div className="flex gap-2">
            {([
              { value: "dark" as const, label: "🌙 深色", desc: "默认暗色主题" },
              { value: "light" as const, label: "☀️ 浅色", desc: "明亮简洁风格" },
              { value: "auto" as const, label: "🔄 跟随系统", desc: "自动适配 OS 主题" },
            ]).map((opt) => (
              <button
                key={opt.value}
                onClick={() => onSetThemeMode(opt.value)}
                className={cn(
                  "flex-1 flex flex-col items-center gap-1 p-3 rounded-lg border transition-all",
                  themeMode === opt.value
                    ? "border-primary bg-primary/10 text-primary"
                    : "border-border hover:border-primary/30"
                )}
              >
                <span className="text-sm font-medium">{opt.label}</span>
                <span className="text-[10px] text-muted-foreground">{opt.desc}</span>
              </button>
            ))}
          </div>
        </CardContent>
      </Card>

      {/* Account Management */}
      <Card>
        <CardHeader className="flex-row justify-between items-center mb-4">
          <CardTitle className="text-sm">🔑 智能体云端账户授权管理器</CardTitle>
          <Button size="sm" variant="outline" onClick={onAddAccount}>
            <Plus className="h-3 w-3" /> 新增账户
          </Button>
        </CardHeader>
        <CardContent>
          {accounts.length === 0 ? (
            <div className="py-2.5 text-center text-muted-foreground text-xs">暂无账户凭证</div>
          ) : (
            <div className="flex flex-col gap-2.5">
              {accounts.map((acc) => (
                <div
                  key={acc.id}
                  className={cn(
                    "flex justify-between items-center p-2.5 border-b border-white/[0.01] rounded-md",
                    acc.is_active && "bg-accent/[0.04]"
                  )}
                >
                  <div>
                    <span className="font-semibold text-sm">{acc.account_name}</span>
                    <span className="text-xs text-muted-foreground ml-2.5">
                      Endpoint: <code>{acc.api_host}</code> | Model: <code>{acc.target_model}</code>
                    </span>
                  </div>
                  <div className="flex gap-1.5">
                    {!acc.is_active && (
                      <Button size="sm" variant="outline" onClick={() => onSwitchAccount(acc.id)}>启用</Button>
                    )}
                    <Button size="sm" variant="outline" onClick={() => onEditAccount(acc)}>
                      <Edit className="h-3 w-3" />
                    </Button>
                    <Button size="sm" variant="outline" onClick={() => onDeleteAccount(acc.id)}>
                      <Trash2 className="h-3 w-3 text-destructive" />
                    </Button>
                  </div>
                </div>
              ))}
            </div>
          )}
        </CardContent>
      </Card>

      {/* System Configuration */}
      <Card>
        <CardContent className="p-5 flex flex-col gap-3">
          <div className="space-y-3">
            <div className="space-y-1.5">
              <Label>本地 HTTP 网关代理端口</Label>
              <Input value={proxyPort} onChange={(e) => setProxyPort(e.target.value)} />
            </div>
            <div className="space-y-1.5">
              <Label>默认全局大模型名称</Label>
              <Input value={targetModel} onChange={(e) => setTargetModel(e.target.value)} placeholder="例如: deepseek-chat 或 gpt-4o" />
            </div>
            <div className="space-y-1.5">
              <Label>默认全局 API 基准地址</Label>
              <Input value={apiHost} onChange={(e) => setApiHost(e.target.value)} />
            </div>
            <div className="space-y-1.5">
              <Label>默认全局 API 密钥</Label>
              <Input type="password" value={apiKey} onChange={(e) => setApiKey(e.target.value)} />
            </div>
          </div>

          <div className="grid grid-cols-2 gap-4 my-2.5">
            <div className="flex items-center gap-2.5">
              <Switch checked={gpuAcceleration} onCheckedChange={setGpuAcceleration} id="gpu_chk" />
              <Label htmlFor="gpu_chk" className="m-0">启用本地 LLM 硬件 GPU 加速</Label>
            </div>
            <div className="space-y-1.5">
              <Label>智能体进程超时时间 (分钟)</Label>
              <Input type="number" value={idleTimeout} onChange={(e) => setIdleTimeout(e.target.value)} />
            </div>
          </div>

          <div className="grid grid-cols-2 gap-4">
            <div className="flex items-center gap-2.5">
              <Switch checked={autoStart} onCheckedChange={setAutoStart} id="autostart_chk" />
              <Label htmlFor="autostart_chk" className="m-0">跟随 Windows 开机自启动</Label>
            </div>
            <div className="flex items-center gap-2.5">
              <Switch checked={startToTray} onCheckedChange={setStartToTray} id="tray_chk" />
              <Label htmlFor="tray_chk" className="m-0">启动时最小化至系统托盘</Label>
            </div>
          </div>

          <Separator className="my-2.5" />

          <div className="grid grid-cols-2 gap-4">
            <div className="flex items-center gap-2.5">
              <Switch checked={useWsl} onCheckedChange={setUseWsl} id="wsl_chk" />
              <Label htmlFor="wsl_chk" className="m-0">在 WSL 中启动</Label>
            </div>
            {useWsl && (
              <div className="space-y-1.5">
                <Label>WSL 发行版名称</Label>
                <Input value={wslDistro} onChange={(e) => setWslDistro(e.target.value)} />
              </div>
            )}
          </div>

          <Button className="w-full mt-4" onClick={onSaveSettings}>
            <Save className="h-4 w-4" /> 💾 保存系统配置并重载网关
          </Button>
        </CardContent>
      </Card>

      {/* ── Selection Assistant ─────────────────────── */}
      <Card>
        <CardHeader>
          <CardTitle className="text-sm flex items-center gap-2">
            <MousePointerClick className="h-4 w-4" /> 🖱️ 划词助手（系统级）
          </CardTitle>
        </CardHeader>
        <CardContent className="flex flex-col gap-3">
          <div className="space-y-1.5">
            <Label>全局快捷键</Label>
            <Input
              value={selectionShortcut}
              onChange={(e) => onSetSelectionShortcut(e.target.value)}
              placeholder="例如: Ctrl+Alt+C"
            />
            <span className="text-[10px] text-muted-foreground">
              在任意应用中选中文字后按下快捷键，自动捕获并送入 QuickAssistant
            </span>
          </div>

          <div className="space-y-1.5">
            <Label>捕获模式</Label>
            <select
              className="w-full border border-border rounded-md px-3 py-2 text-sm bg-background"
              value={selectionCaptureMode}
              onChange={(e) => onSetSelectionCaptureMode(e.target.value)}
            >
              <option value="hybrid">混合模式（UIA 优先，自动回退剪贴板）</option>
              <option value="uia_only">仅 UI Automation（被动读取，不修改剪贴板）</option>
              <option value="clipboard_only">仅剪贴板（模拟 Ctrl+C）</option>
            </select>
          </div>

          <div className="grid grid-cols-2 gap-4">
            <div className="flex items-center gap-2.5">
              <Switch checked={selectionShowOnCapture} onCheckedChange={onSetSelectionShowOnCapture} id="sel_show_qa" />
              <Label htmlFor="sel_show_qa" className="m-0">捕获后自动弹出 QA</Label>
            </div>
            <div className="flex items-center gap-2.5">
              <Switch checked={selectionPreserveClipboard} onCheckedChange={onSetSelectionPreserveClipboard} id="sel_preserve_cb" />
              <Label htmlFor="sel_preserve_cb" className="m-0">保护剪贴板原内容</Label>
            </div>
          </div>

          <div className="flex gap-2 mt-1">
            <Button
              size="sm"
              variant="outline"
              onClick={async () => {
                const text = await onTestSelectionCapture();
                if (text) {
                  toast.success("捕获成功！选中文本: " + text.slice(0, 60) + (text.length > 60 ? "…" : ""));
                } else {
                  toast.error("未捕获到文本，请确保有文字被选中");
                }
              }}
              disabled={isSelectionCapturing}
            >
              {isSelectionCapturing ? "捕获中..." : "🎯 测试捕获"}
            </Button>
            <Button
              size="sm"
              variant="default"
              onClick={async () => {
                try {
                  await onSaveSelectionSettings({
                    shortcut: selectionShortcut,
                    captureMode: selectionCaptureMode,
                    showOnCapture: selectionShowOnCapture,
                    preserveClipboard: selectionPreserveClipboard,
                  });
                  toast.success("划词助手设置已保存！快捷键已热重载。");
                } catch (e) {
                  toast.error("保存失败：" + String(e));
                }
              }}
            >
              <Save className="h-3 w-3" /> 保存设置
            </Button>
          </div>

          {selectionCaptureError && (
            <div className="text-xs text-destructive bg-destructive/10 rounded px-2 py-1.5 mt-1">
              ⚠️ {selectionCaptureError}
            </div>
          )}
        </CardContent>
      </Card>

      {/* ── Capture History ─────────────────────────── */}
      <Card>
        <CardHeader className="flex-row justify-between items-center">
          <CardTitle className="text-sm flex items-center gap-2">
            <Clock className="h-4 w-4" /> 📋 捕获历史
          </CardTitle>
          <div className="flex gap-2">
            <Button size="sm" variant="outline" onClick={onLoadSelectionHistory}>
              <RefreshCw className="h-3 w-3" /> 刷新
            </Button>
            {selectionHistory.length > 0 && (
              <Button size="sm" variant="outline" onClick={onClearSelectionHistory}>
                <Trash2 className="h-3 w-3 text-destructive" /> 清空
              </Button>
            )}
          </div>
        </CardHeader>
        <CardContent>
          {selectionHistory.length === 0 ? (
            <div className="py-3 text-center text-muted-foreground text-xs">
              暂无捕获记录，选中文字后按 {selectionShortcut} 试试
            </div>
          ) : (
            <div className="flex flex-col gap-1.5 max-h-64 overflow-y-auto">
              {selectionHistory.map((entry) => (
                <div
                  key={entry.id}
                  className="flex items-start gap-2 p-2 border border-white/[0.03] rounded-md hover:bg-white/[0.02]"
                >
                  <div className="flex-1 min-w-0">
                    <div className="flex items-center gap-2 mb-0.5">
                      <Badge variant="outline" className="text-[9px] px-1 py-0">
                        {entry.source === "uia" ? "UIA" : "剪贴板"}
                      </Badge>
                      <span className="text-[10px] text-muted-foreground truncate">
                        {entry.window_title || "未知窗口"}
                      </span>
                      <span className="text-[9px] text-muted-foreground/60">
                        {new Date(entry.created_at).toLocaleTimeString()}
                      </span>
                    </div>
                    <pre className="text-xs text-foreground/80 truncate whitespace-nowrap overflow-hidden">
                      {entry.captured_text.slice(0, 120)}{entry.captured_text.length > 120 ? "…" : ""}
                    </pre>
                  </div>
                  <div className="flex gap-1 flex-shrink-0">
                    <button
                      className="p-1 hover:bg-white/10 rounded"
                      title="复制"
                      onClick={() => {
                        navigator.clipboard.writeText(entry.captured_text);
                        toast.success("已复制到剪贴板");
                      }}
                    >
                      <Copy className="h-3 w-3" />
                    </button>
                    <button
                      className="p-1 hover:bg-white/10 rounded"
                      title="删除"
                      onClick={() => onDeleteSelectionHistoryItem(entry.id)}
                    >
                      <Trash2 className="h-3 w-3 text-destructive" />
                    </button>
                  </div>
                </div>
              ))}
            </div>
          )}
        </CardContent>
      </Card>

      {/* ── Translation Settings ────────────────────── */}
      <Card>
        <CardHeader>
          <CardTitle className="text-sm flex items-center gap-2">
            <Languages className="h-4 w-4" /> 🌐 AI 翻译助手
          </CardTitle>
        </CardHeader>
        <CardContent className="flex flex-col gap-3">
          <div className="grid grid-cols-2 gap-4">
            <div className="space-y-1.5">
              <Label>首选目标语言</Label>
              <select
                className="w-full border border-border rounded-md px-3 py-2 text-sm bg-background"
                value={translatePreferredLang}
                onChange={(e) => onSetTranslatePreferredLang(e.target.value)}
              >
                {BUILTIN_LANGUAGES.map(l => (
                  <option key={l.langCode} value={l.langCode}>{l.emoji} {l.value}</option>
                ))}
              </select>
            </div>
            <div className="space-y-1.5">
              <Label>备选目标语言</Label>
              <select
                className="w-full border border-border rounded-md px-3 py-2 text-sm bg-background"
                value={translateAlterLang}
                onChange={(e) => onSetTranslateAlterLang(e.target.value)}
              >
                {BUILTIN_LANGUAGES.map(l => (
                  <option key={l.langCode} value={l.langCode}>{l.emoji} {l.value}</option>
                ))}
              </select>
            </div>
          </div>

          <div className="flex items-center gap-2 mt-1 text-xs text-muted-foreground bg-muted/30 rounded-md px-3 py-2">
            <ArrowRightLeft className="h-3 w-3 flex-shrink-0" />
            <span>智能双向：识别为首选语言时自动翻译为备选，反之亦然</span>
          </div>

          <div className="space-y-1.5">
            <Label>翻译模型（留空使用全局默认）</Label>
            <Input
              value={translateModel}
              onChange={(e) => onSetTranslateModel(e.target.value)}
              placeholder="例如: deepseek-chat 或 gpt-4o"
            />
          </div>

          <div className="flex items-center gap-2.5">
            <Switch checked={translateAutoDetect} onCheckedChange={onSetTranslateAutoDetect} id="translate_auto_detect" />
            <Label htmlFor="translate_auto_detect" className="m-0">自动检测源语言</Label>
          </div>

          <div className="space-y-1.5">
            <Label>自定义翻译 Prompt（留空使用默认）</Label>
            <textarea
              className="w-full border border-border rounded-md px-3 py-2 text-xs bg-background min-h-20 font-mono"
              value={translateCustomPrompt}
              onChange={(e) => onSetTranslateCustomPrompt(e.target.value)}
              placeholder="可用占位符: {{target_language}}, {{text}}"
            />
          </div>

          <Button
            size="sm"
            variant="default"
            onClick={async () => {
              try {
                await onSaveTranslationSettings({
                  preferredLang: translatePreferredLang,
                  alterLang: translateAlterLang,
                  translateModel,
                  autoDetect: translateAutoDetect,
                  customPrompt: translateCustomPrompt,
                });
                toast.success("翻译设置已保存！");
              } catch (e) {
                toast.error("保存失败：" + String(e));
              }
            }}
          >
            <Save className="h-3 w-3" /> 保存翻译设置
          </Button>
        </CardContent>
      </Card>

      {/* ── Search Providers ─────────────────────── */}
      <Card>
        <CardHeader>
          <CardTitle className="text-sm flex items-center gap-2">
            <Search className="h-4 w-4" /> 🔍 网络搜索配置
          </CardTitle>
        </CardHeader>
        <CardContent className="flex flex-col gap-3">
          {searchProviders.length === 0 ? (
            <div className="text-xs text-muted-foreground text-center py-2">暂无搜索引擎配置</div>
          ) : (
            <div className="flex flex-col gap-2">
              {searchProviders.map((sp) => (
                <div key={sp.id} className="flex justify-between items-center p-2 rounded-md border-b border-white/[0.01]">
                  <div className="flex items-center gap-2">
                    <Badge variant={sp.is_enabled ? "default" : "secondary"}>
                      {sp.api_type.toUpperCase()}
                    </Badge>
                    <span className="text-sm font-medium">{sp.name}</span>
                    {sp.api_address && (
                      <span className="text-xs text-muted-foreground"><code>{sp.api_address}</code></span>
                    )}
                  </div>
                  <div className="flex gap-1.5">
                    <Button size="sm" variant="outline" onClick={() => onEditSearchProvider(sp)}>
                      <Edit className="h-3 w-3" />
                    </Button>
                    <Button size="sm" variant="outline" onClick={() => onDeleteSearchProvider(sp.id)}>
                      <Trash2 className="h-3 w-3 text-destructive" />
                    </Button>
                  </div>
                </div>
              ))}
            </div>
          )}
        </CardContent>
      </Card>
    </div>
  );
}

// ── MCP Servers Sub-Tab ─────────────────────────────────

function McpSubTab({
  mcpServers,
  onOpenMcpModal,
  onDeleteMcpServer,
  showMcpModal,
  editingMcpServer,
  mcpForm,
  onCloseMcpModal,
  onUpdateMcpForm,
  onSaveMcpServer,
}: SettingsTabProps) {
  return (
    <div className="flex flex-col gap-4 max-w-[650px]">
      <Card>
        <CardHeader className="flex-row justify-between items-center mb-4">
          <CardTitle className="text-sm">🔌 MCP 服务器管理</CardTitle>
          <Button size="sm" variant="outline" onClick={() => onOpenMcpModal()}>
            <Plus className="h-3 w-3" /> 新增
          </Button>
        </CardHeader>
        <CardContent>
          {mcpServers.length === 0 ? (
            <div className="py-2.5 text-center text-muted-foreground text-xs">
              暂无 MCP 服务器配置。点击"新增"添加。
            </div>
          ) : (
            <div className="flex flex-col gap-2.5">
              {mcpServers.map((srv) => (
                <div
                  key={srv.id}
                  className="flex justify-between items-center p-2.5 border-b border-white/[0.01] rounded-md"
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
                  <div className="flex gap-1.5">
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

      {/* MCP Server Modal */}
      {showMcpModal && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
          <div className="glass-card p-6 w-[480px] max-h-[80vh] overflow-y-auto">
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

function BackupSubTab({
  backupTableInfo,
  backupSelectedTables,
  isBackupExporting,
  isBackupImporting,
  lastImportResult,
  onToggleBackupTable,
  onSelectAllBackupTables,
  onDeselectAllBackupTables,
  onExportBackup,
  onImportBackup,
}: SettingsTabProps) {
  const [importJson, setImportJson] = useState("");

  const handleExport = async () => {
    const json = await onExportBackup();
    if (json) {
      // Create a downloadable blob
      const blob = new Blob([json], { type: "application/json" });
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = url;
      a.download = `omnix-backup-${new Date().toISOString().slice(0, 10)}.json`;
      a.click();
      URL.revokeObjectURL(url);
      toast.success("备份导出成功！");
    } else {
      toast.error("导出失败");
    }
  };

  const handleImport = async () => {
    if (!importJson.trim()) {
      toast.error("请粘贴或选择备份 JSON 内容");
      return;
    }
    const result = await onImportBackup(importJson);
    if (result) {
      toast.success(`恢复成功！共 ${result.total_rows} 行数据。`);
    } else {
      toast.error("恢复失败，请检查 JSON 格式。");
    }
  };

  const handleFileSelect = () => {
    const input = document.createElement("input");
    input.type = "file";
    input.accept = ".json";
    input.onchange = async (e) => {
      const file = (e.target as HTMLInputElement).files?.[0];
      if (file) {
        const text = await file.text();
        setImportJson(text);
      }
    };
    input.click();
  };

  return (
    <div className="flex flex-col gap-4 max-w-[650px]">
      {/* Export */}
      <Card>
        <CardHeader>
          <CardTitle className="text-sm flex items-center gap-2">
            <Download className="h-4 w-4" /> 📦 数据导出
          </CardTitle>
        </CardHeader>
        <CardContent className="flex flex-col gap-3">
          <div className="flex justify-between items-center mb-1">
            <span className="text-xs text-muted-foreground">选择要导出的数据表：</span>
            <div className="flex gap-1">
              <Button size="sm" variant="ghost" onClick={onSelectAllBackupTables}>全选</Button>
              <Button size="sm" variant="ghost" onClick={onDeselectAllBackupTables}>全不选</Button>
            </div>
          </div>
          <div className="grid grid-cols-2 gap-1.5 max-h-[240px] overflow-y-auto">
            {backupTableInfo.map((t) => (
              <label key={t.table_name} className="flex items-center gap-2 text-xs p-1.5 rounded hover:bg-muted/50 cursor-pointer">
                <Checkbox
                  checked={backupSelectedTables.has(t.table_name)}
                  onCheckedChange={() => onToggleBackupTable(t.table_name)}
                />
                <span className="flex-1 truncate">{t.table_name}</span>
                <Badge variant="secondary" className="text-[10px]">{t.row_count}</Badge>
              </label>
            ))}
          </div>
          <Button className="w-full" onClick={handleExport} disabled={isBackupExporting || backupSelectedTables.size === 0}>
            {isBackupExporting ? "导出中…" : <><Download className="h-4 w-4" /> 导出备份</>}
          </Button>
        </CardContent>
      </Card>

      {/* Import */}
      <Card>
        <CardHeader>
          <CardTitle className="text-sm flex items-center gap-2">
            <Upload className="h-4 w-4" /> 📥 数据恢复
          </CardTitle>
        </CardHeader>
        <CardContent className="flex flex-col gap-3">
          <div className="text-xs text-warning bg-warning/10 p-2 rounded">
            ⚠️ 恢复操作将覆盖选中表的现有数据，请谨慎操作！
          </div>
          <Button variant="outline" onClick={handleFileSelect}>
            选择备份文件 (.json)
          </Button>
          <textarea
            className="w-full h-32 text-xs p-2 rounded-md border border-border bg-muted/30 font-mono resize-y"
            placeholder="或直接粘贴备份 JSON 内容…"
            value={importJson}
            onChange={(e) => setImportJson(e.target.value)}
          />
          <Button
            className="w-full"
            onClick={handleImport}
            disabled={isBackupImporting || !importJson.trim()}
          >
            {isBackupImporting ? "恢复中…" : <><Upload className="h-4 w-4" /> 恢复数据</>}
          </Button>
          {lastImportResult && (
            <div className="text-xs bg-success/10 text-success p-2 rounded">
              ✅ 恢复完成：{lastImportResult.tables_restored.map(([t, c]) => `${t}(${c}行)`).join(", ")}，共 {lastImportResult.total_rows} 行
            </div>
          )}
        </CardContent>
      </Card>
    </div>
  );
}

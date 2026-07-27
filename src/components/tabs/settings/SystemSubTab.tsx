/** Split from SettingsTab.tsx — pure move, no behavior change. */
import { useEffect, useState } from "react";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { Separator } from "@/components/ui/separator";
import { Badge } from "@/components/ui/badge";
import { ArrowRightLeft, Edit, ExternalLink, FileText, Key, Languages, MousePointerClick, Plus, Save, Search, Settings, Trash2 } from "lucide-react";
import { cn } from "@/lib/utils";
import { toast } from "@/components/ui/sonner";
import { modelApi, statusDockApi } from "@/lib/tauri-api";
import { openUrl } from "@tauri-apps/plugin-opener";
import { BUILTIN_LANGUAGES } from "@/lib/translate-constants";
import type { SettingsTabProps } from "./types";

function StatusDockToggle() {
  const [on, setOn] = useState(false);
  useEffect(() => {
    statusDockApi.isEnabled().then(setOn).catch(() => {});
  }, []);
  const toggle = async (next: boolean) => {
    setOn(next);
    try {
      await statusDockApi.setEnabled(next);
      toast.success(next ? "悬浮状态坞已开启（下次开机也会显示）" : "悬浮状态坞已关闭（不再开机自启）");
    } catch (e) {
      setOn(!next);
      toast.error(`切换失败：${e}`);
    }
  };
  return (
    <div className="flex items-center gap-2.5">
      <Switch checked={on} onCheckedChange={(v) => void toggle(v)} id="statusdock_chk" />
      <Label htmlFor="statusdock_chk" className="m-0">显示悬浮状态坞（默认关，不开机自启）</Label>
    </div>
  );
}

export function SystemSubTab({
  accounts,
  onAddAccount,
  onEditAccount,
  onDeleteAccount,
  onSwitchAccount,
  targetModel, setTargetModel,
  gpuAcceleration, setGpuAcceleration,
  idleTimeout, setIdleTimeout,
  autoStart, setAutoStart,
  startToTray, setStartToTray,
  useWsl, setUseWsl,
  wslDistro, setWslDistro,
  onSaveSettings,
  selectionCaptureMode, onSetSelectionCaptureMode,
  selectionAutoCaptureEnabled, onSetSelectionAutoCaptureEnabled,
  selectionPreserveClipboard, onSetSelectionPreserveClipboard,
  isSelectionCapturing,
  lastSelectionCapture: _lastSelectionCapture,
  selectionCaptureError,
  selectionHistory: _selectionHistory,
  onTestSelectionCapture,
  onSaveSelectionSettings: _onSaveSelectionSettings,
  onLoadSelectionHistory: _onLoadSelectionHistory,
  onDeleteSelectionHistoryItem: _onDeleteSelectionHistoryItem,
  onClearSelectionHistory: _onClearSelectionHistory,
  translatePreferredLang, onSetTranslatePreferredLang,
  translateAlterLang, onSetTranslateAlterLang,
  translateModel, onSetTranslateModel,
  translateAutoDetect, onSetTranslateAutoDetect,
  translateCustomPrompt, onSetTranslateCustomPrompt,
  onSaveTranslationSettings,
  themeMode,
  onSetThemeMode,
  searchProviders,
  onAddSearchProvider,
  onEditSearchProvider,
  onDeleteSearchProvider,
  showSearchProviderModal,
  editingSearchProvider: _editingSearchProvider,
  searchProviderForm,
  onCloseSearchProviderModal,
  onUpdateSearchProviderForm,
  onSaveSearchProvider,
}: SettingsTabProps) {
  // ── Available models for dropdowns ────────────────────
  const [availableModels, setAvailableModels] = useState<string[]>([]);

  useEffect(() => {
    modelApi.getAvailableNames()
      .then(names => setAvailableModels(names))
      .catch(e => console.error("[Settings] Failed to load available models:", e));
  }, []);

  // ── Left-side group nav: one long mixed page → focused groups ──
  type SystemGroup = "general" | "accounts" | "selection" | "translate" | "docs" | "search";
  const [group, setGroup] = useState<SystemGroup>("general");
  const GROUPS: { id: SystemGroup; label: string; icon: React.ReactNode }[] = [
    { id: "general", label: "常规", icon: <Settings className="h-3.5 w-3.5" /> },
    { id: "accounts", label: "云端账户", icon: <Key className="h-3.5 w-3.5" /> },
    { id: "selection", label: "划词助手", icon: <MousePointerClick className="h-3.5 w-3.5" /> },
    { id: "translate", label: "翻译", icon: <Languages className="h-3.5 w-3.5" /> },
    { id: "docs", label: "文档处理", icon: <FileText className="h-3.5 w-3.5" /> },
    { id: "search", label: "搜索服务", icon: <Search className="h-3.5 w-3.5" /> },
  ];

  return (
    <div className="mx-auto flex w-full max-w-5xl gap-4">
      <nav className="w-36 shrink-0">
        <div className="sticky top-0 flex flex-col gap-1">
          {GROUPS.map((g) => (
            <button
              key={g.id}
              onClick={() => setGroup(g.id)}
              className={cn(
                "flex items-center gap-2 rounded-md px-3 py-2 text-left text-sm transition",
                group === g.id
                  ? "bg-accent/10 text-accent border border-accent/30"
                  : "text-muted-foreground hover:bg-muted/20 hover:text-foreground border border-transparent"
              )}
            >
              {g.icon}
              {g.label}
            </button>
          ))}
        </div>
      </nav>

      <div className="flex min-w-0 flex-1 flex-col gap-4">
      {/* Theme selector lives in the title bar. */}
      {false && (
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
                <span className="text-xs text-muted-foreground">{opt.desc}</span>
              </button>
            ))}
          </div>
        </CardContent>
      </Card>
      )}

      {/* Account Management */}
      {group === "accounts" && (
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
                    "flex justify-between items-center p-2.5 border-b border-border rounded-md",
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
      )}

      {/* System Configuration */}
      {group === "general" && (
      <Card>
        <CardContent className="p-5 flex flex-col gap-3">
          <div className="space-y-3">
            <div className="space-y-1.5">
              <Label>内置功能默认模型</Label>
              <select
                className="w-full border border-border rounded-md px-3 py-2 text-sm bg-background"
                value={targetModel}
                onChange={(e) => setTargetModel(e.target.value)}
              >
                <option value="">— 请选择 —</option>
                {availableModels.map(m => (
                  <option key={m} value={m}>{m}</option>
                ))}
              </select>
              <span className="text-xs text-muted-foreground">
                供 OMNIX 自身的内置功能使用（划词翻译、语言检测、知识库问答等），与 Agent 对话无关。
                Agent（Codex/Claude）默认模型请到「模型中心」用 ☆ 设置。
              </span>
            </div>
          </div>

          <div className="grid grid-cols-1 sm:grid-cols-2 gap-4 my-2.5">
            <div className="flex items-center gap-2.5">
              <Switch checked={gpuAcceleration} onCheckedChange={setGpuAcceleration} id="gpu_chk" />
              <Label htmlFor="gpu_chk" className="m-0">启用本地 LLM 硬件 GPU 加速</Label>
            </div>
            <div className="space-y-1.5">
              <Label>智能体进程超时时间 (分钟)</Label>
              <Input type="number" value={idleTimeout} onChange={(e) => setIdleTimeout(e.target.value)} />
            </div>
          </div>

          <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
            <div className="flex items-center gap-2.5">
              <Switch checked={autoStart} onCheckedChange={setAutoStart} id="autostart_chk" />
              <Label htmlFor="autostart_chk" className="m-0">跟随 Windows 开机自启动</Label>
            </div>
            <div className="flex items-center gap-2.5">
              <Switch checked={startToTray} onCheckedChange={setStartToTray} id="tray_chk" />
              <Label htmlFor="tray_chk" className="m-0">启动时最小化至系统托盘</Label>
            </div>
            <StatusDockToggle />
          </div>

          <Separator className="my-2.5" />

          <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
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
            <Save className="h-4 w-4" /> 保存系统配置并重载网关
          </Button>
        </CardContent>
      </Card>
      )}

      {/* ── Selection Assistant ─────────────────────── */}
      {group === "selection" && (
      <Card>
        <CardHeader>
          <CardTitle className="text-sm flex items-center gap-2">
            <MousePointerClick className="h-4 w-4" /> 划词助手
          </CardTitle>
        </CardHeader>
        <CardContent className="flex flex-col gap-3">
          <div className="bg-muted/30 rounded-md px-3 py-2 text-xs text-muted-foreground flex items-start gap-2">
            <MousePointerClick className="h-3.5 w-3.5 mt-0.5 shrink-0" />
            <span>
              <strong>使用方法：</strong>开启自动捕获后，在任意应用中用鼠标选中文字，QA 窗口会自动弹出操作栏（翻译/解释/总结/搜索）。
              按 Ctrl+Shift+Space 可手动唤起 QA 窗口。
            </span>
          </div>

          <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
            <div className="flex items-center gap-2.5">
              <Switch
                checked={selectionAutoCaptureEnabled}
                onCheckedChange={(v) => onSetSelectionAutoCaptureEnabled(v)}
                id="sel_auto_capture"
              />
              <Label htmlFor="sel_auto_capture" className="m-0">🖱️ 自动捕获选中文字（划词监听）</Label>
            </div>
            <div className="flex items-center gap-2.5">
              <Switch checked={selectionPreserveClipboard} onCheckedChange={onSetSelectionPreserveClipboard} id="sel_preserve_cb" />
              <Label htmlFor="sel_preserve_cb" className="m-0">保护剪贴板原内容</Label>
            </div>
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
          </div>

          {selectionCaptureError && (
            <div className="text-xs text-destructive bg-destructive/10 rounded px-2 py-1.5 mt-1">
              ⚠️ {selectionCaptureError}
            </div>
          )}
        </CardContent>
      </Card>
      )}

      {/* ── Translation Settings ────────────────────── */}
      {group === "translate" && (
      <Card>
        <CardHeader>
          <CardTitle className="text-sm flex items-center gap-2">
            <Languages className="h-4 w-4" /> AI 翻译助手
          </CardTitle>
        </CardHeader>
        <CardContent className="flex flex-col gap-3">
          <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
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
            <select
              className="w-full border border-border rounded-md px-3 py-2 text-sm bg-background"
              value={translateModel}
              onChange={(e) => onSetTranslateModel(e.target.value)}
            >
              <option value="">— 使用全局默认 —</option>
              {availableModels.map(m => (
                <option key={m} value={m}>{m}</option>
              ))}
            </select>
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
      )}

      {/* ── Document Processing ────────────────────── */}
      {group === "docs" && (
      <Card>
        <CardHeader>
          <CardTitle className="text-sm flex items-center gap-2">
            <FileText className="h-4 w-4" /> 文档处理
          </CardTitle>
        </CardHeader>
        <CardContent className="flex flex-col gap-3">
          <span className="text-xs text-muted-foreground">
            配置文档处理器后，导入 PDF/Word 等文件时可自动转换为 Markdown 再入库，提高检索精度。
          </span>

          <div className="space-y-1.5">
            <Label>文档转 Markdown 处理器</Label>
            <select
              className="w-full border border-border rounded-md px-3 py-2 text-sm bg-background"
              defaultValue="system_ocr"
            >
              <option value="system_ocr">系统 OCR（Windows 内置）</option>
              <option value="tesseract">Tesseract OCR（需安装）</option>
              <option value="mineru">MinerU API</option>
              <option value="doc2x">Doc2X API</option>
              <option value="mistral_ocr">Mistral OCR API</option>
              <option value="paddleocr">PaddleOCR API</option>
            </select>
          </div>

          <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
            <div className="space-y-1.5">
              <Label>API 地址（可选）</Label>
              <Input placeholder="https://api.example.com/v1/convert" />
            </div>
            <div className="space-y-1.5">
              <Label>API Key（可选）</Label>
              <Input type="password" placeholder="留空则无需认证" />
            </div>
          </div>

          <div className="flex items-center gap-2.5">
            <Switch defaultChecked={true} id="doc_auto_convert" />
            <Label htmlFor="doc_auto_convert" className="m-0">导入文档时自动转换为 Markdown</Label>
          </div>

          <div className="bg-muted/30 rounded-md px-3 py-2 text-xs text-muted-foreground">
            💡 <strong>提示：</strong>本地 OCR 无需额外配置；API 处理器需填写地址和密钥。
            推荐使用 Doc2X 或 MinerU 以获得最佳转换质量。
          </div>
        </CardContent>
      </Card>
      )}

      {/* ── Search Providers ─────────────────────── */}
      {group === "search" && (
      <Card>
        <CardHeader className="flex-row justify-between items-center">
          <CardTitle className="text-sm flex items-center gap-2">
            <Search className="h-4 w-4" /> 网络搜索配置
          </CardTitle>
          <Button size="sm" variant="outline" onClick={onAddSearchProvider}>
            <Plus className="h-3 w-3" /> 新增
          </Button>
        </CardHeader>
        <CardContent className="flex flex-col gap-3">
          <span className="text-xs text-muted-foreground">
            配置网络搜索引擎后，在智能体对话中启用"联网搜索"即可让 AI 获取实时网络信息。
          </span>

          {/* Preset search providers */}
          <div>
            <span className="text-xs font-medium text-muted-foreground mb-2 block">🌐 搜索引擎</span>
            <div className="grid grid-cols-2 sm:grid-cols-3 gap-1.5">
              {([
                { name: "Google", type: "google", url: "https://developers.google.com/custom-search", color: "text-blue-400", desc: "Custom Search API" },
                { name: "Bing", type: "bing", url: "https://www.microsoft.com/en-us/bing/apis", color: "text-cyan-400", desc: "Bing Search API" },
                { name: "DuckDuckGo", type: "duckduckgo", url: "https://duckduckgo.com/api", color: "text-orange-400", desc: "免费免 Key" },
                { name: "SearXNG", type: "searxng", url: "https://docs.searxng.org", color: "text-amber-400", desc: "自托管元搜索" },
              ]).map(preset => (
                <button
                  key={preset.name}
                  className="flex items-start gap-1.5 p-2 rounded-md border border-border/30 bg-background/20 hover:bg-background/40 transition-all text-left cursor-pointer"
                  onClick={() => {
                    onUpdateSearchProviderForm("api_type", preset.type);
                    onUpdateSearchProviderForm("name", preset.name);
                    if (preset.type === "searxng") {
                      onUpdateSearchProviderForm("api_address", "http://localhost:8080");
                    } else if (preset.type === "google") {
                      onUpdateSearchProviderForm("api_address", "");
                    }
                    openUrl(preset.url).catch(() => window.open(preset.url, "_blank"));
                  }}
                >
                  <ExternalLink className={cn("h-3 w-3 shrink-0 mt-0.5", preset.color)} />
                  <div className="flex flex-col">
                    <span className="text-xs font-medium">{preset.name}</span>
                    <span className="text-xs text-muted-foreground leading-tight">{preset.desc}</span>
                  </div>
                </button>
              ))}
            </div>
          </div>
          <div>
            <span className="text-xs font-medium text-muted-foreground mb-2 block">☁️ 云端搜索</span>
            <div className="grid grid-cols-2 sm:grid-cols-3 gap-1.5">
              {([
                { name: "Tavily", type: "tavily", url: "https://tavily.com", color: "text-blue-400" },
                { name: "Exa", type: "exa", url: "https://exa.ai", color: "text-emerald-400" },
                { name: "智谱搜索", type: "zhipu", url: "https://open.bigmodel.cn", color: "text-violet-400" },
                { name: "Bocha", type: "bocha", url: "https://bocha.ai", color: "text-amber-400" },
                { name: "Jina", type: "jina", url: "https://jina.ai", color: "text-cyan-400" },
              ]).map(preset => (
                <button
                  key={preset.name}
                  className="flex items-center gap-1.5 p-2 rounded-md border border-border/30 bg-background/20 hover:bg-background/40 transition-all text-left cursor-pointer"
                  onClick={() => {
                    onUpdateSearchProviderForm("api_type", preset.type);
                    onUpdateSearchProviderForm("name", preset.name);
                    openUrl(preset.url).catch(() => window.open(preset.url, "_blank"));
                  }}
                >
                  <ExternalLink className={cn("h-3 w-3 shrink-0", preset.color)} />
                  <span className="text-xs font-medium">{preset.name}</span>
                </button>
              ))}
            </div>
          </div>
          {searchProviders.length === 0 ? (
            <div className="text-xs text-muted-foreground text-center py-2">暂无搜索引擎配置，点击"新增"添加</div>
          ) : (
            <div className="flex flex-col gap-2">
              {searchProviders.map((sp) => (
                <div key={sp.id} className="flex justify-between items-center p-2 rounded-md border-b border-border">
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
      )}
      {/* Search Provider Modal */}
      {showSearchProviderModal && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
          <div className="glass-card p-6 w-full max-w-[480px] mx-4 max-h-[80vh] overflow-y-auto">
            <h3 className="text-lg font-semibold mb-4">
              {_editingSearchProvider ? "编辑搜索引擎" : "新增搜索引擎"}
            </h3>
            <div className="flex flex-col gap-3">
              <div className="space-y-1.5">
                <Label>名称</Label>
                <Input value={searchProviderForm.name} onChange={(e) => onUpdateSearchProviderForm("name", e.target.value)} placeholder="例如: SearXNG" />
              </div>
              <div className="space-y-1.5">
                <Label>类型</Label>
                <div className="flex flex-wrap gap-1.5">
                  {(["google", "bing", "duckduckgo", "searxng", "brave", "tavily", "exa", "zhipu", "bocha", "jina"] as const).map((t) => (
                    <button
                      key={t}
                      onClick={() => onUpdateSearchProviderForm("api_type", t)}
                      className={cn(
                        "px-2.5 py-1.5 rounded-md border text-xs",
                        searchProviderForm.api_type === t ? "border-primary bg-primary/10" : "border-border"
                      )}
                    >
                      {t.toUpperCase()}
                    </button>
                  ))}
                </div>
              </div>
              <div className="space-y-1.5">
                <Label>{searchProviderForm.api_type === "searxng" ? "SearXNG 地址" : "API 地址"}</Label>
                <Input value={searchProviderForm.api_address} onChange={(e) => onUpdateSearchProviderForm("api_address", e.target.value)} placeholder={searchProviderForm.api_type === "searxng" ? "http://localhost:8080" : "https://api.example.com"} />
                {searchProviderForm.api_type === "searxng" && (
                  <span className="text-xs text-muted-foreground">自托管 SearXNG 实例地址，通常为 Docker 部署的 localhost:8080</span>
                )}
              </div>
              {/* SearXNG: no API key needed, show Basic Auth instead */}
              {searchProviderForm.api_type === "searxng" ? (
                <div className="space-y-1.5">
                  <Label>HTTP Basic Auth（可选）</Label>
                  <div className="flex gap-2">
                    <Input value={searchProviderForm.api_key || ""} onChange={(e) => onUpdateSearchProviderForm("api_key", e.target.value)} placeholder="用户名" className="flex-1" />
                    <Input type="password" value={(searchProviderForm as Record<string, unknown>).basicAuthPassword as string || ""} onChange={(e) => onUpdateSearchProviderForm("basicAuthPassword" as keyof typeof searchProviderForm, e.target.value as never)} placeholder="密码" className="flex-1" />
                  </div>
                  <span className="text-xs text-muted-foreground">适用于远程部署的 SearXNG 实例（RFC 7617 Basic 认证）</span>
                </div>
              ) : (
                <div className="space-y-1.5">
                  <Label>API Key（可选）</Label>
                  <Input type="password" value={searchProviderForm.api_key} onChange={(e) => onUpdateSearchProviderForm("api_key", e.target.value)} placeholder="留空则无需认证" />
                </div>
              )}
              <div className="flex items-center gap-2">
                <Switch checked={searchProviderForm.is_enabled} onCheckedChange={(v) => onUpdateSearchProviderForm("is_enabled", v)} id="sp_enabled" />
                <Label htmlFor="sp_enabled">启用</Label>
              </div>
            </div>
            <div className="flex justify-end gap-2 mt-4">
              <Button variant="ghost" onClick={onCloseSearchProviderModal}>取消</Button>
              <Button variant="outline" onClick={async () => {
                try {
                  // Save first so the provider exists for testing
                  await onSaveSearchProvider();
                  const { search: searchFn } = await import("@/lib/tauri-api").then(m => m.searchApi);
                  const results = await searchFn("hello world", searchProviderForm.id, 3);
                  toast.success(`连通成功！返回 ${results.length} 条结果`);
                } catch (e) {
                  toast.error("连通失败：" + String(e));
                }
              }}>
                <Search className="h-4 w-4" /> 测试连接
              </Button>
              <Button onClick={async () => {
                try {
                  await onSaveSearchProvider();
                  toast.success("搜索引擎保存成功！");
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
    </div>
  );
}

// ── MCP Servers Sub-Tab ─────────────────────────────────


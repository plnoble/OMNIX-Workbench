/** Split from SettingsTab.tsx — pure move, no behavior change. */
import { useEffect, useState } from "react";
import { useSettingsStore } from "@/store/AppStore";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { Separator } from "@/components/ui/separator";
import { FileText, Save, Settings } from "lucide-react";
import { cn } from "@/lib/utils";
import { toast } from "@/components/ui/sonner";
import { modelApi, statusDockApi } from "@/lib/tauri-api";
import type { AvailableModel } from "@/types";

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

export function SystemSubTab() {
  const s = useSettingsStore();
  const {
    targetModel, setTargetModel,
    gpuAcceleration, setGpuAcceleration,
    idleTimeout, setIdleTimeout,
    autoStart, setAutoStart,
    startToTray, setStartToTray,
    useWsl, setUseWsl,
    wslDistro, setWslDistro,
    themeMode,
    setThemeMode: onSetThemeMode,
  } = s;
  // 这段提示原本在 App.tsx 的 `handleSaveSettings` 里。搬 store 时如果只取
  // `s.saveSettings`，保存成功/失败的反馈就会静默消失——所以连提示一起搬过来，
  // 放在触发它的地方。
  const onSaveSettings = async () => {
    try {
      await s.saveSettings();
      toast.success("设置保存成功！中转代理网关已热重载，外部 Agent 配置文件已同步。");
    } catch (e) {
      toast.error("保存设置失败：" + e);
    }
  };
  // ── Available models for dropdowns ────────────────────
  const [availableModels, setAvailableModels] = useState<AvailableModel[]>([]);

  useEffect(() => {
    modelApi.getAvailable()
      .then(setAvailableModels)
      .catch(e => console.error("[Settings] Failed to load available models:", e));
  }, []);

  // ── Left-side group nav: one long mixed page → focused groups ──
  type SystemGroup = "general" | "selection" | "translate" | "docs";
  const [group, setGroup] = useState<SystemGroup>("general");
  const GROUPS: { id: SystemGroup; label: string; icon: React.ReactNode }[] = [
    { id: "general", label: "常规", icon: <Settings className="h-3.5 w-3.5" /> },
    { id: "docs", label: "文档处理", icon: <FileText className="h-3.5 w-3.5" /> },
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

      {/* 「云端账户」组已删除：它和「智能体 → 账号凭据」是同一张 agent_accounts
          表、同一个弹窗、同一份数据，只是这里平铺、看不出哪个账号属于哪个
          agent，误启用的代价还很高（活跃账号会覆盖网关的目标模型）。 */}

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
                  <option key={m.id} value={m.id}>
                    {m.model_name} · {m.platform_name}
                  </option>
                ))}
              </select>
              <span className="text-xs text-muted-foreground">
                供 OMNIX 自身的内置功能使用（划词翻译、语言检测、知识库问答等），与 Agent 对话无关。
                Agent（Codex/Claude）默认模型请到「模型中心」用 ☆ 设置。
              </span>
              {/* 同名模型注册在多个平台上时，只存裸名字网关就得靠哈希二选一，
                  挑中不支持它的那个就是「Model does not exist」。这里存的是
                  platform_id:model_name，选项也带上平台名，让你看得见选的是哪个。 */}
              {availableModels.some((m) => m.ambiguous) && (
                <span className="text-xs text-warning">
                  ⚠️ 有模型名同时注册在多个平台上（
                  {[...new Set(availableModels.filter((m) => m.ambiguous).map((m) => m.model_name))].join("、")}
                  ）。请按平台选清楚；旧的设置里存的是裸名字，建议重新选一次。
                </span>
              )}
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

      {/* ── Translation Settings ────────────────────── */}

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

      {/* 搜索配置已整体搬到宫格的「搜索」页：那里本来就有供应商列表和调试搜索，
          配 Key 却要跑到设置里来，是同一张表被切成两半。现在一页做完。 */}
      </div>
    </div>
  );
}

// ── MCP Servers Sub-Tab ─────────────────────────────────


/** Split from SettingsTab.tsx — pure move, no behavior change. */
import { useEffect, useState } from "react";
import { useSettingsStore } from "@/store/AppStore";
import { Card, CardContent } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { Save } from "lucide-react";
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

  /* 这里以前有一条左侧分组导航（常规 / 划词 / 翻译 / 文档处理）。四个组陆续
     搬走或删掉之后只剩「常规」一个，导航整条留着——而「文档处理」那一项**点进去
     是一片空白**：卡片删了，导航项没跟着删。

     这是这个项目反复出现的那一类：读的一半迁走了，写的一半留着。表现不是报错，
     是一个点了没反应的入口——比缺功能更糟，用户会以为是自己没配好。

     所以整条导航一起删。一个组的导航是个假选择。各组现在的去处：
     - 划词、翻译、搜索：都在宫格里各自的页
     - 文档处理：在「Office」页。OfficeCLI 的安装、版本、引擎状态本来就在那里
       （`officeApi.status()` / `install()`），配置跑到设置里来是同一张表切两半 */

  return (
    <div className="mx-auto flex w-full max-w-5xl gap-4">
      <div className="flex min-w-0 flex-1 flex-col gap-4">
      {/* 「外观主题」那张卡片已删除：主题选择器在标题栏，这里是它被搬走之后
          留下的一整块 `{false && ...}` —— 永远不渲染，但每次读这个文件的人都得
          先跳过它，而且它是唯一让 cn / CardHeader / themeMode 看起来还有人用的
          地方。留着不害人（没有入口指向它），但它把「这个文件还依赖什么」这件事
          说谎了。 */}

      {/* 「云端账户」组已删除：它和「智能体 → 账号凭据」是同一张 agent_accounts
          表、同一个弹窗、同一份数据，只是这里平铺、看不出哪个账号属于哪个
          agent，误启用的代价还很高（活跃账号会覆盖网关的目标模型）。 */}

      {/* System Configuration */}
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

          {/* 这里以前有「在 WSL 中启动」开关和发行版输入框。它们是布景：
              `useSettings` 的 load/save 两侧都没有 `use_wsl`，拨了刷新就没，
              后端那两条 WSL 分支（起 agent、网关绑 0.0.0.0）从没生效过。
              连同后端一起删了——留一个假开关，等于给下一个人埋一个
              「顺手修好开关就打开局域网无鉴权入口」的坑。 */}

          <Button className="w-full mt-4" onClick={onSaveSettings}>
            <Save className="h-4 w-4" /> 保存系统配置并重载网关
          </Button>
        </CardContent>
      </Card>

      {/* ── Selection Assistant ─────────────────────── */}

      {/* ── Translation Settings ────────────────────── */}

      {/* 这里以前有一整张「文档处理」卡片：转换器下拉（系统 OCR / Tesseract /
          MinerU / Doc2X / Mistral OCR / PaddleOCR）、API 地址、API Key、
          「导入时自动转换」开关，还附一句「推荐 Doc2X 或 MinerU 以获得最佳
          转换质量」。

          整块是布景：select 用 `defaultValue`、两个 Input 只有 placeholder、
          Switch 用 `defaultChecked`——没有 state、没有 onChange、没有保存、
          没有后端。选了等于没选。

          假控件比缺功能更糟：缺功能用户知道没有，假控件让人以为是自己没配好。
          真要做文档转换，从后端命令开始，不是从下拉框开始。 */}

      {/* 搜索配置已整体搬到宫格的「搜索」页：那里本来就有供应商列表和调试搜索，
          配 Key 却要跑到设置里来，是同一张表被切成两半。现在一页做完。 */}
      </div>
    </div>
  );
}

// ── MCP Servers Sub-Tab ─────────────────────────────────


/**
 * RemoteDevTab — 远程开发 (Labs)。
 *
 * 把家里的 Linux 服务器接进来：
 * ① 远程模型主机测试（Ollama/vLLM）→ 测通后去模型中心添加，全软件用上远端显卡
 * ② SSH 主机管理 + 连接测试（用系统 ssh，继承 ~/.ssh/config 与密钥）
 * ③ 远端硬件/Agent CLI 探测与安装
 * ④ 运行测试台：headless 会话跑在远端，Claude 经 `ssh -R` 回连本机网关
 *   （技能正式池注入/模型路由照常生效）。Labs 验证稳定后再接入主对话。
 */
import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  Activity,
  Cpu,
  FlaskConical,
  HardDrive,
  Loader2,
  Play,
  Plus,
  Server,
  Square,
  Trash2,
  Wifi,
} from "lucide-react";
import { toast } from "sonner";

import { cn } from "@/lib/utils";
import {
  remoteDevApi,
  type RemoteAgentStatus,
  type RemoteHardware,
  type SshHost,
} from "@/lib/tauri-api";

const EMPTY_HOST: SshHost = {
  id: "",
  name: "",
  host: "",
  port: 22,
  user: "",
  key_path: "",
  default_workdir: "",
};

const RUN_AGENTS = ["Claude Code", "Codex", "Grok Build"];

export function RemoteDevTab() {
  const [hosts, setHosts] = useState<SshHost[]>([]);
  const [selectedId, setSelectedId] = useState<string>("");
  const [form, setForm] = useState<SshHost>(EMPTY_HOST);
  const [showForm, setShowForm] = useState(false);
  const [busy, setBusy] = useState<string | null>(null);

  const [hw, setHw] = useState<RemoteHardware | null>(null);
  const [agents, setAgents] = useState<RemoteAgentStatus[] | null>(null);

  const [modelUrl, setModelUrl] = useState("http://192.168.1.10:11434/v1");
  const [modelTest, setModelTest] = useState<string>("");

  // 运行测试台
  const [runAgent, setRunAgent] = useState("Claude Code");
  const [workdir, setWorkdir] = useState("");
  const [prompt, setPrompt] = useState("");
  const [useGateway, setUseGateway] = useState(true);
  const [runId, setRunId] = useState<string | null>(null);
  const [output, setOutput] = useState("");
  const outRef = useRef<HTMLPreElement | null>(null);

  const selected = hosts.find((h) => h.id === selectedId) ?? null;

  const load = useCallback(async () => {
    try {
      const list = await remoteDevApi.listHosts();
      setHosts(list);
      if (list.length > 0 && !list.some((h) => h.id === selectedId)) {
        setSelectedId(list[0].id);
      }
    } catch (e) {
      toast.error(String(e));
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selectedId]);

  useEffect(() => {
    void load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // 运行输出流
  useEffect(() => {
    const un1 = listen<{ run_id: string; line: string }>("remote-run-output", (e) => {
      setOutput((prev) => prev + e.payload.line + "\n");
      requestAnimationFrame(() => outRef.current?.scrollTo(0, outRef.current.scrollHeight));
    });
    const un2 = listen<{ run_id: string; code: number }>("remote-run-done", (e) => {
      setRunId(null);
      setOutput((prev) => prev + `\n—— 运行结束（exit ${e.payload.code}）——\n`);
    });
    return () => {
      void un1.then((f) => f());
      void un2.then((f) => f());
    };
  }, []);

  // 切换主机时带出默认工作目录
  useEffect(() => {
    if (selected) setWorkdir(selected.default_workdir);
    setHw(null);
    setAgents(null);
  }, [selectedId]); // eslint-disable-line react-hooks/exhaustive-deps

  const saveHost = async () => {
    try {
      const saved = await remoteDevApi.saveHost(form);
      toast.success(`已保存主机「${saved.name || saved.host}」`);
      setShowForm(false);
      setForm(EMPTY_HOST);
      await load();
      setSelectedId(saved.id);
    } catch (e) {
      toast.error(String(e));
    }
  };

  const testHost = async (id: string) => {
    setBusy(`test:${id}`);
    try {
      const r = await remoteDevApi.testHost(id);
      if (r.ok) toast.success(`连接成功（${r.latency_ms}ms）：${r.uname.slice(0, 60)}`);
      else toast.error(`连接失败：${r.error || "未知错误"}`);
    } catch (e) {
      toast.error(String(e));
    } finally {
      setBusy(null);
    }
  };

  const probe = async () => {
    if (!selected) return;
    setBusy("probe");
    try {
      setHw(await remoteDevApi.probeHardware(selected.id));
    } catch (e) {
      toast.error(String(e));
    } finally {
      setBusy(null);
    }
  };

  const detectAgents = async () => {
    if (!selected) return;
    setBusy("agents");
    try {
      setAgents(await remoteDevApi.detectAgents(selected.id));
    } catch (e) {
      toast.error(String(e));
    } finally {
      setBusy(null);
    }
  };

  const installAgent = async (agent: string) => {
    if (!selected) return;
    setBusy(`install:${agent}`);
    try {
      await remoteDevApi.installAgent(selected.id, agent);
      toast.success(`${agent} 已在远端安装`);
      await detectAgents();
    } catch (e) {
      toast.error(String(e));
    } finally {
      setBusy(null);
    }
  };

  const testModel = async () => {
    setBusy("model");
    setModelTest("");
    try {
      const r = await remoteDevApi.testModelHost(modelUrl);
      setModelTest(
        r.ok
          ? `✅ 连通（${r.latency_ms}ms）· ${r.models.length} 个模型：${r.models.slice(0, 6).join("、")}${r.models.length > 6 ? "…" : ""}\n→ 去「模型中心」新增平台，地址填 ${modelUrl}，即可全软件使用这台机器的模型。`
          : `❌ 不通（${r.latency_ms}ms）：${r.error}\n提示：跨公网访问建议用 Tailscale 或 SSH 隧道（ssh -L 11434:127.0.0.1:11434 服务器）。`,
      );
    } catch (e) {
      toast.error(String(e));
    } finally {
      setBusy(null);
    }
  };

  const startRun = async () => {
    if (!selected) {
      toast.error("先添加并选择一台 SSH 主机");
      return;
    }
    setOutput("");
    try {
      const r = await remoteDevApi.startRun(selected.id, runAgent, workdir, prompt, useGateway);
      setRunId(r.run_id);
    } catch (e) {
      toast.error(String(e));
    }
  };

  const stopRun = async () => {
    if (runId) {
      await remoteDevApi.stopRun(runId).catch(() => {});
      setRunId(null);
    }
  };

  return (
    <div className="flex h-full flex-1 flex-col overflow-hidden bg-background">
      <div className="flex items-center gap-2 border-b border-border px-6 py-4">
        <Server className="h-5 w-5 text-primary" />
        <div>
          <div className="flex items-center gap-2 text-lg font-semibold">
            远程开发
            <span className="inline-flex items-center gap-1 rounded bg-warning/15 px-1.5 py-0.5 text-[10px] font-normal text-warning">
              <FlaskConical className="h-3 w-3" /> Labs
            </span>
          </div>
          <p className="text-xs text-muted-foreground">
            连接家里的 Linux 服务器：远端显卡跑模型、远端跑 agent 会话（Claude 经反向隧道回连本机网关，技能/路由照常生效）
          </p>
        </div>
      </div>

      <div className="flex flex-col gap-4 overflow-y-auto p-6">
        {/* ① 远程模型主机 (P0) */}
        <div className="rounded-xl border border-border bg-card/40 p-4">
          <div className="flex items-center gap-2 text-sm font-semibold">
            <Activity className="h-4 w-4 text-primary" /> 远程模型主机（Ollama / vLLM）
          </div>
          <div className="mt-2 flex gap-2">
            <input
              value={modelUrl}
              onChange={(e) => setModelUrl(e.target.value)}
              placeholder="http://服务器IP:11434/v1"
              className="h-9 flex-1 rounded-lg border border-border bg-background px-3 font-mono text-sm outline-none focus:border-primary"
            />
            <button
              onClick={() => void testModel()}
              disabled={busy === "model"}
              className="inline-flex h-9 items-center gap-1.5 rounded-lg bg-primary px-4 text-sm font-medium text-primary-foreground hover:opacity-90 disabled:opacity-50"
            >
              {busy === "model" ? <Loader2 className="h-4 w-4 animate-spin" /> : <Wifi className="h-4 w-4" />}
              测试连通
            </button>
          </div>
          {modelTest && (
            <pre className="mt-2 whitespace-pre-wrap rounded-lg bg-muted/20 p-2.5 text-xs leading-5">{modelTest}</pre>
          )}
        </div>

        {/* ② SSH 主机 (P1) */}
        <div className="rounded-xl border border-border bg-card/40 p-4">
          <div className="flex items-center gap-2 text-sm font-semibold">
            <Server className="h-4 w-4 text-primary" /> SSH 主机
            <button
              onClick={() => { setShowForm((v) => !v); setForm(EMPTY_HOST); }}
              className="ml-auto inline-flex h-7 items-center gap-1 rounded-md border border-border px-2 text-xs hover:bg-muted/40"
            >
              <Plus className="h-3 w-3" /> 添加主机
            </button>
          </div>
          {showForm && (
            <div className="mt-2 grid grid-cols-2 gap-2 rounded-lg border border-border p-3 md:grid-cols-3">
              {(
                [
                  ["name", "名称（如 家里服务器）"],
                  ["host", "地址（IP / 域名 / ssh config 别名）"],
                  ["user", "用户名"],
                  ["key_path", "私钥路径（留空=系统 ssh 配置）"],
                  ["default_workdir", "默认工作目录（如 /home/me/proj）"],
                ] as [keyof SshHost, string][]
              ).map(([key, ph]) => (
                <input
                  key={key}
                  value={String(form[key] ?? "")}
                  onChange={(e) => setForm((f) => ({ ...f, [key]: e.target.value }))}
                  placeholder={ph}
                  className="h-8 rounded-md border border-border bg-background px-2 text-xs outline-none focus:border-primary"
                />
              ))}
              <input
                type="number"
                value={form.port}
                onChange={(e) => setForm((f) => ({ ...f, port: Number(e.target.value) || 22 }))}
                placeholder="端口 22"
                className="h-8 rounded-md border border-border bg-background px-2 text-xs outline-none focus:border-primary"
              />
              <button
                onClick={() => void saveHost()}
                className="col-span-2 h-8 rounded-md bg-primary text-xs font-medium text-primary-foreground hover:opacity-90 md:col-span-3"
              >
                保存
              </button>
            </div>
          )}
          <div className="mt-2 flex flex-col gap-1.5">
            {hosts.length === 0 && !showForm && (
              <p className="text-xs text-muted-foreground">还没有主机——点「添加主机」。Windows 需已启用 OpenSSH 客户端（Win10+ 默认自带）。</p>
            )}
            {hosts.map((h) => (
              <div
                key={h.id}
                onClick={() => setSelectedId(h.id)}
                className={cn(
                  "flex cursor-pointer flex-wrap items-center gap-2 rounded-lg border px-3 py-2",
                  selectedId === h.id ? "border-primary bg-primary/5" : "border-border bg-background/60",
                )}
              >
                <span className="text-sm font-medium">{h.name || h.host}</span>
                <span className="font-mono text-xs text-muted-foreground">
                  {h.user ? `${h.user}@` : ""}{h.host}:{h.port}
                </span>
                <span className="ml-auto flex items-center gap-1">
                  <button
                    onClick={(e) => { e.stopPropagation(); void testHost(h.id); }}
                    disabled={busy !== null}
                    className="inline-flex h-7 items-center gap-1 rounded-md border border-border px-2 text-xs hover:bg-muted/40 disabled:opacity-50"
                  >
                    {busy === `test:${h.id}` ? <Loader2 className="h-3 w-3 animate-spin" /> : <Wifi className="h-3 w-3" />}
                    测试
                  </button>
                  <button
                    onClick={(e) => {
                      e.stopPropagation();
                      if (window.confirm(`删除主机「${h.name || h.host}」？`)) {
                        void remoteDevApi.deleteHost(h.id).then(load);
                      }
                    }}
                    className="rounded p-1 text-muted-foreground hover:text-destructive"
                  >
                    <Trash2 className="h-3 w-3" />
                  </button>
                </span>
              </div>
            ))}
          </div>
        </div>

        {/* ③ 远端探测 (P2) */}
        {selected && (
          <div className="rounded-xl border border-border bg-card/40 p-4">
            <div className="flex flex-wrap items-center gap-2 text-sm font-semibold">
              <Cpu className="h-4 w-4 text-primary" /> 远端环境 · {selected.name || selected.host}
              <div className="ml-auto flex gap-1.5">
                <button
                  onClick={() => void probe()}
                  disabled={busy !== null}
                  className="inline-flex h-7 items-center gap-1 rounded-md border border-border px-2 text-xs hover:bg-muted/40 disabled:opacity-50"
                >
                  {busy === "probe" ? <Loader2 className="h-3 w-3 animate-spin" /> : <Cpu className="h-3 w-3" />}
                  探测硬件
                </button>
                <button
                  onClick={() => void detectAgents()}
                  disabled={busy !== null}
                  className="inline-flex h-7 items-center gap-1 rounded-md border border-border px-2 text-xs hover:bg-muted/40 disabled:opacity-50"
                >
                  {busy === "agents" ? <Loader2 className="h-3 w-3 animate-spin" /> : <HardDrive className="h-3 w-3" />}
                  检测 Agent CLI
                </button>
              </div>
            </div>
            {hw && (
              <div className="mt-2 grid grid-cols-3 gap-2 text-center">
                {[
                  ["GPU", hw.gpu],
                  ["内存", hw.ram_mb > 0 ? `${(hw.ram_mb / 1024).toFixed(1)} GB` : "未知"],
                  ["CPU", hw.cpu_cores > 0 ? `${hw.cpu_cores} 核` : "未知"],
                ].map(([label, v]) => (
                  <div key={label} className="rounded-lg border border-border bg-background/60 p-2">
                    <div className="truncate text-sm font-medium" title={String(v)}>{v}</div>
                    <div className="text-[10px] text-muted-foreground">{label}</div>
                  </div>
                ))}
              </div>
            )}
            {agents && (
              <div className="mt-2 flex flex-col gap-1">
                {agents.map((a) => (
                  <div key={a.agent} className="flex items-center gap-2 rounded-md border border-border/60 px-2 py-1.5 text-xs">
                    <span className="font-medium">{a.agent}</span>
                    {a.installed ? (
                      <>
                        <span className="rounded bg-success/15 px-1.5 py-0.5 text-[10px] text-success">已装</span>
                        <span className="truncate font-mono text-[11px] text-muted-foreground" title={a.path}>{a.path}</span>
                        <span className="ml-auto shrink-0 text-[10px] text-muted-foreground">{a.version}</span>
                      </>
                    ) : (
                      <>
                        <span className="rounded bg-muted/60 px-1.5 py-0.5 text-[10px] text-muted-foreground">未装</span>
                        <button
                          onClick={() => void installAgent(a.agent)}
                          disabled={busy !== null}
                          className="ml-auto rounded border border-border px-2 py-0.5 text-[11px] hover:bg-muted/40 disabled:opacity-50"
                        >
                          {busy === `install:${a.agent}` ? "安装中…" : "npm 安装"}
                        </button>
                      </>
                    )}
                  </div>
                ))}
              </div>
            )}
          </div>
        )}

        {/* ④ 运行测试台 (P1) */}
        {selected && (
          <div className="rounded-xl border border-border bg-card/40 p-4">
            <div className="flex items-center gap-2 text-sm font-semibold">
              <Play className="h-4 w-4 text-primary" /> 远程运行测试台
            </div>
            <div className="mt-2 flex flex-wrap items-center gap-2">
              <select
                value={runAgent}
                onChange={(e) => setRunAgent(e.target.value)}
                className="h-9 rounded-lg border border-border bg-background px-2 text-sm"
              >
                {RUN_AGENTS.map((a) => (
                  <option key={a} value={a}>{a}</option>
                ))}
              </select>
              <input
                value={workdir}
                onChange={(e) => setWorkdir(e.target.value)}
                placeholder="远端工作目录（可空）"
                className="h-9 min-w-52 flex-1 rounded-lg border border-border bg-background px-3 font-mono text-sm outline-none focus:border-primary"
              />
              <label
                className="flex cursor-pointer items-center gap-1.5 text-xs text-muted-foreground"
                title="Claude 经 ssh -R 回连本机网关：技能正式池注入/模型路由/多账号在远端会话同样生效"
              >
                <input
                  type="checkbox"
                  checked={useGateway}
                  onChange={(e) => setUseGateway(e.target.checked)}
                  className="h-3.5 w-3.5"
                />
                回连本机网关
              </label>
            </div>
            <div className="mt-2 flex gap-2">
              <textarea
                value={prompt}
                onChange={(e) => setPrompt(e.target.value)}
                rows={2}
                placeholder="要在远端执行的任务，例如：查看这个仓库的结构并总结主要模块"
                className="flex-1 resize-none rounded-lg border border-border bg-background px-3 py-2 text-sm outline-none focus:border-primary"
              />
              {runId ? (
                <button
                  onClick={() => void stopRun()}
                  className="inline-flex h-auto items-center gap-1.5 rounded-lg border border-destructive/40 bg-destructive/10 px-4 text-sm text-destructive hover:bg-destructive/20"
                >
                  <Square className="h-4 w-4" /> 停止
                </button>
              ) : (
                <button
                  onClick={() => void startRun()}
                  disabled={!prompt.trim()}
                  className="inline-flex h-auto items-center gap-1.5 rounded-lg bg-primary px-4 text-sm font-medium text-primary-foreground hover:opacity-90 disabled:opacity-50"
                >
                  <Play className="h-4 w-4" /> 运行
                </button>
              )}
            </div>
            {(output || runId) && (
              <pre
                ref={outRef}
                className="mt-2 max-h-72 overflow-auto whitespace-pre-wrap rounded-lg bg-muted/20 p-3 font-mono text-xs leading-5"
              >
                {output || "等待远端输出…"}
              </pre>
            )}
            <p className="mt-1.5 text-[10px] text-muted-foreground">
              Codex / Grok Build 使用远端已配置的凭据；Claude Code 勾选「回连本机网关」后走 OMNIX 网关（远端 sshd 需允许端口转发，默认开启）。
            </p>
          </div>
        )}
      </div>
    </div>
  );
}

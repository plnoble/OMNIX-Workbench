/**
 * AuthCenterTab — 认证中心 (sub2api inspired).
 *
 * Log in to your Claude / OpenAI-Codex / Gemini subscription via the standard
 * PKCE browser flow, store the tokens encrypted, and use them in any agent
 * through CLI takeover. You authenticate in your own browser and paste the code
 * back — OMNIX never sees your password.
 */
import { useCallback, useEffect, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { KeyRound, ExternalLink, Trash2, RefreshCw, ShieldCheck, ShieldAlert, Terminal, RotateCcw } from "lucide-react";

import { Button } from "@/components/ui/button";
import { toast } from "@/components/ui/sonner";
import { cn } from "@/lib/utils";
import {
  oauthApi, cliTakeoverApi,
  type OAuthProvider, type OAuthAccountView, type OAuthStartResult,
  type AgentTakeoverState, type TakeoverTarget,
} from "@/lib/tauri-api";

const TAKEOVER_AGENTS: { id: string; name: string }[] = [
  { id: "claude_code", name: "Claude Code" },
  { id: "codex", name: "Codex" },
  { id: "gemini", name: "Gemini CLI" },
];

const PROVIDERS: { id: OAuthProvider; name: string; hint: string }[] = [
  { id: "anthropic_claude", name: "Claude 订阅", hint: "claude.ai 登录，授权后页面会显示一段码" },
  { id: "openai_codex", name: "OpenAI / Codex", hint: "auth.openai.com 登录，浏览器会跳到 localhost，复制整条地址回来" },
  { id: "google_gemini", name: "Google Gemini", hint: "Google 登录，授权后页面会显示一段码" },
];

export function AuthCenterTab() {
  const [accounts, setAccounts] = useState<OAuthAccountView[]>([]);
  const [pending, setPending] = useState<{ provider: OAuthProvider; start: OAuthStartResult } | null>(null);
  const [code, setCode] = useState("");
  const [label, setLabel] = useState("");
  const [busy, setBusy] = useState<string>("");
  // CLI takeover
  const [takeover, setTakeover] = useState<AgentTakeoverState[]>([]);
  const [targetValue, setTargetValue] = useState("gateway");
  const [pickedAgents, setPickedAgents] = useState<string[]>(["claude_code"]);
  const [confirmApply, setConfirmApply] = useState(false);

  const load = useCallback(async () => {
    try {
      const [accs, tk] = await Promise.all([oauthApi.listAccounts(), cliTakeoverApi.status()]);
      setAccounts(accs);
      setTakeover(tk);
    } catch {
      /* transient */
    }
  }, []);

  useEffect(() => { void load(); }, [load]);

  const toValue = (): TakeoverTarget =>
    targetValue.startsWith("oauth:")
      ? { kind: "oauth", ref_id: targetValue.slice("oauth:".length) }
      : { kind: "gateway" };

  const applyTakeover = async () => {
    setConfirmApply(false);
    if (pickedAgents.length === 0) { toast.error("请先选择要接管的 Agent"); return; }
    setBusy("takeover-apply");
    try {
      const reports = await cliTakeoverApi.apply(pickedAgents, toValue());
      await load();
      toast.success(`已接管 ${reports.length} 个 Agent 的配置（原配置已备份）`);
    } catch (error) {
      toast.error(`接管失败：${String(error)}`);
    } finally {
      setBusy("");
    }
  };

  const revertTakeover = async (agent: string) => {
    setBusy(`revert-${agent}`);
    try {
      await cliTakeoverApi.revert(agent);
      await load();
      toast.success("已从备份还原");
    } catch (error) {
      toast.error(`还原失败：${String(error)}`);
    } finally {
      setBusy("");
    }
  };

  const beginLogin = async (provider: OAuthProvider) => {
    setBusy(provider);
    try {
      const start = await oauthApi.start(provider);
      setPending({ provider, start });
      setCode("");
      setLabel("");
      await openUrl(start.authorize_url);
    } catch (error) {
      toast.error(`发起登录失败：${String(error)}`);
    } finally {
      setBusy("");
    }
  };

  const completeLogin = async () => {
    if (!pending || !code.trim()) return;
    setBusy("complete");
    try {
      await oauthApi.complete(pending.provider, code.trim(), label.trim());
      toast.success("已连接订阅账号");
      setPending(null);
      setCode("");
      setLabel("");
      await load();
    } catch (error) {
      toast.error(`授权失败：${String(error)}`);
    } finally {
      setBusy("");
    }
  };

  const refresh = async (id: string) => {
    setBusy(id);
    try {
      await oauthApi.refreshAccount(id);
      await load();
      toast.success("已刷新令牌");
    } catch (error) {
      toast.error(`刷新失败：${String(error)}`);
    } finally {
      setBusy("");
    }
  };

  const remove = async (id: string) => {
    setBusy(id);
    try {
      await oauthApi.deleteAccount(id);
      await load();
      toast.success("已移除账号");
    } catch (error) {
      toast.error(`移除失败：${String(error)}`);
    } finally {
      setBusy("");
    }
  };

  return (
    <div className="flex h-full flex-1 flex-col overflow-hidden bg-background">
      <div className="border-b border-border px-6 py-4">
        <div className="flex items-center gap-2 text-lg font-semibold">
          <KeyRound className="h-5 w-5 text-primary" /> 认证中心
        </div>
        <p className="mt-1 max-w-3xl text-sm leading-6 text-muted-foreground">
          登录你自己的 Claude / OpenAI / Gemini 订阅，令牌加密保存，再经「CLI 接管」在各 Agent 里使用。
          你在自己的浏览器里完成登录，OMNIX 不接触你的账号密码。
        </p>
      </div>

      <div className="flex flex-col gap-5 overflow-y-auto p-6">
        {/* ToS notice */}
        <div className="flex items-start gap-2 rounded-lg border border-warning/30 bg-warning/5 p-3 text-xs leading-5 text-muted-foreground">
          <ShieldAlert className="mt-0.5 h-4 w-4 shrink-0 text-warning" />
          <span>
            用订阅账号驱动第三方工具，可能受各供应商服务条款约束。请自行确认合规使用，风险自负。
          </span>
        </div>

        {/* Login buttons */}
        <div className="grid grid-cols-1 gap-3 sm:grid-cols-3">
          {PROVIDERS.map((provider) => (
            <div key={provider.id} className="rounded-lg border border-border bg-card/40 p-4">
              <div className="text-sm font-semibold">{provider.name}</div>
              <p className="mt-1 mb-3 text-xs leading-5 text-muted-foreground">{provider.hint}</p>
              <Button
                size="sm"
                variant="outline"
                className="w-full"
                disabled={busy === provider.id}
                onClick={() => void beginLogin(provider.id)}
              >
                <ExternalLink className="h-4 w-4" /> 登录并授权
              </Button>
            </div>
          ))}
        </div>

        {/* Paste-code step */}
        {pending && (
          <div className="rounded-lg border border-primary/40 bg-primary/5 p-4">
            <div className="mb-2 text-sm font-semibold">
              完成「{PROVIDERS.find((p) => p.id === pending.provider)?.name}」授权
            </div>
            <p className="mb-3 text-xs leading-5 text-muted-foreground">
              浏览器已打开授权页。{pending.start.manual_paste
                ? "授权后页面会显示一段授权码，复制粘贴到下面。"
                : `授权后浏览器会跳到 ${pending.start.redirect_uri}（可能显示打不开），把地址栏里那条完整链接复制粘贴到下面。`}
              没自动打开？<button className="text-primary underline" onClick={() => void openUrl(pending.start.authorize_url)}>手动打开</button>。
            </p>
            <input
              type="text"
              value={code}
              onChange={(e) => setCode(e.target.value)}
              placeholder={pending.start.manual_paste ? "粘贴授权码" : "粘贴完整回调链接或授权码"}
              className="mb-2 w-full rounded-md border border-border bg-background px-3 py-2 text-sm"
              onKeyDown={(e) => { if (e.key === "Enter") void completeLogin(); }}
            />
            <input
              type="text"
              value={label}
              onChange={(e) => setLabel(e.target.value)}
              placeholder="账号备注（可选，如 工作号 / 个人号）"
              className="mb-3 w-full rounded-md border border-border bg-background px-3 py-2 text-sm"
            />
            <div className="flex justify-end gap-2">
              <Button size="sm" variant="ghost" onClick={() => setPending(null)}>取消</Button>
              <Button size="sm" disabled={!code.trim() || busy === "complete"} onClick={() => void completeLogin()}>
                完成连接
              </Button>
            </div>
          </div>
        )}

        {/* Account cards */}
        <div>
          <div className="mb-2 text-sm font-semibold">已连接账号</div>
          {accounts.length === 0 ? (
            <div className="rounded-lg border border-dashed border-border py-8 text-center text-sm text-muted-foreground">
              还没有连接任何订阅账号。
            </div>
          ) : (
            <div className="space-y-2">
              {accounts.map((account) => (
                <div key={account.id} className="flex items-center gap-3 rounded-lg border border-border p-3">
                  <div className={cn("flex h-8 w-8 shrink-0 items-center justify-center rounded-full", account.expired ? "bg-destructive/15" : "bg-success/15")}>
                    {account.expired
                      ? <ShieldAlert className="h-4 w-4 text-destructive" />
                      : <ShieldCheck className="h-4 w-4 text-success" />}
                  </div>
                  <div className="min-w-0 flex-1">
                    <div className="flex items-center gap-2">
                      <span className="text-sm font-medium">{account.label}</span>
                      <span className="rounded bg-muted/30 px-1.5 py-0.5 text-xs text-muted-foreground">{account.provider_name}</span>
                    </div>
                    <div className="mt-0.5 text-xs text-muted-foreground">
                      {account.expired ? "令牌已过期，请刷新" : account.expires_at ? `有效至 ${account.expires_at}` : "长期有效"}
                      {account.has_refresh ? " · 可自动续期" : " · 无刷新令牌"}
                    </div>
                  </div>
                  {account.has_refresh && (
                    <button
                      type="button"
                      title="刷新令牌"
                      disabled={busy === account.id}
                      onClick={() => void refresh(account.id)}
                      className="rounded border border-border p-1.5 text-muted-foreground hover:bg-muted/30 hover:text-foreground disabled:opacity-50"
                    >
                      <RefreshCw className={cn("h-3.5 w-3.5", busy === account.id && "animate-spin")} />
                    </button>
                  )}
                  <button
                    type="button"
                    title="移除账号"
                    disabled={busy === account.id}
                    onClick={() => void remove(account.id)}
                    className="rounded border border-border p-1.5 text-muted-foreground hover:bg-destructive/10 hover:text-destructive disabled:opacity-50"
                  >
                    <Trash2 className="h-3.5 w-3.5" />
                  </button>
                </div>
              ))}
            </div>
          )}
        </div>

        {/* ── CLI takeover ─────────────────────────────────────── */}
        <div className="rounded-lg border border-border bg-card/40 p-4">
          <div className="mb-1 flex items-center gap-2 text-sm font-semibold">
            <Terminal className="h-4 w-4 text-primary" /> 在 CLI 里使用（配置接管）
          </div>
          <p className="mb-3 text-xs leading-5 text-muted-foreground">
            一键把所选 Agent 的<strong>原生配置</strong>指向目标（改动在 OMNIX 之外的终端也生效）。
            写前自动备份，可随时「还原」。
          </p>

          {/* Target */}
          <div className="mb-3">
            <div className="mb-1 text-xs font-medium">目标</div>
            <select
              value={targetValue}
              onChange={(e) => setTargetValue(e.target.value)}
              className="h-9 w-full rounded-md border border-border bg-background px-2 text-sm"
            >
              <option value="gateway">OMNIX 网关（本机代理，统一路由/熔断/计量）</option>
              {accounts.map((account) => (
                <option key={account.id} value={`oauth:${account.id}`}>
                  订阅：{account.label}（{account.provider_name}）
                </option>
              ))}
            </select>
          </div>

          {/* Agents */}
          <div className="mb-3 flex flex-col gap-2">
            {TAKEOVER_AGENTS.map((agent) => {
              const state = takeover.find((t) => t.agent === agent.id);
              const picked = pickedAgents.includes(agent.id);
              return (
                <div key={agent.id} className="flex items-center gap-2 rounded-md border border-border/60 px-3 py-2">
                  <input
                    type="checkbox"
                    checked={picked}
                    onChange={(e) =>
                      setPickedAgents((prev) =>
                        e.target.checked ? [...prev, agent.id] : prev.filter((a) => a !== agent.id),
                      )
                    }
                  />
                  <div className="min-w-0 flex-1">
                    <div className="text-sm font-medium">{agent.name}</div>
                    <div className="truncate text-xs text-muted-foreground">
                      {state?.current_base_url ? `当前指向 ${state.current_base_url}` : "当前使用 CLI 自身配置"}
                    </div>
                  </div>
                  {state?.has_backup && (
                    <button
                      type="button"
                      title="从备份还原原配置"
                      disabled={busy === `revert-${agent.id}`}
                      onClick={() => void revertTakeover(agent.id)}
                      className="flex items-center gap-1 rounded border border-border px-2 py-1 text-xs text-muted-foreground hover:bg-muted/30 hover:text-foreground disabled:opacity-50"
                    >
                      <RotateCcw className="h-3 w-3" /> 还原
                    </button>
                  )}
                </div>
              );
            })}
          </div>

          <Button
            size="sm"
            disabled={busy === "takeover-apply" || pickedAgents.length === 0}
            onClick={() => setConfirmApply(true)}
          >
            应用接管
          </Button>
        </div>
      </div>

      {/* Confirm apply — writes real config files */}
      {confirmApply && (
        <div className="fixed inset-0 z-[1000] flex items-center justify-center bg-black/60 p-4 backdrop-blur-sm">
          <div className="w-full max-w-md rounded-lg border border-border bg-card p-5 shadow-xl">
            <h3 className="m-0 mb-2 text-base font-semibold text-foreground">确认接管这些 Agent 的配置？</h3>
            <p className="mb-1 text-sm text-muted-foreground">
              将修改 {pickedAgents.map((a) => TAKEOVER_AGENTS.find((t) => t.id === a)?.name).join("、")} 的原生配置文件。
            </p>
            <p className="mb-4 text-xs leading-5 text-muted-foreground">
              这些是 CLI 的真实配置，改动在 OMNIX 之外也生效。原文件会先自动备份，之后可用每个 Agent 的「还原」按钮回退。
            </p>
            <div className="flex justify-end gap-2">
              <Button size="sm" variant="ghost" onClick={() => setConfirmApply(false)}>取消</Button>
              <Button size="sm" onClick={() => void applyTakeover()}>确认接管</Button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

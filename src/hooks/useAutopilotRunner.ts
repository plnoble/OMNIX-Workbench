/**
 * useAutopilotRunner — executes queued autopilot runs (Multica-inspired).
 *
 * The backend scheduler enqueues a run (a conversation + `queued` row) when an
 * autopilot is due. This hook polls for queued runs and executes each exactly
 * once through the real runtime — starting a session with the agent's default
 * model and sending the prompt — so the result is a normal, reviewable
 * conversation. It never steals focus; the run just appears in the list.
 */
import { useEffect } from "react";
import { toast } from "sonner";

import { autopilotApi, runtimeApi, type QueuedAutopilotRun } from "@/lib/tauri-api";
import { getRuntimeAgentId } from "@/lib/agentRegistry";
import type { RuntimePermissionPolicy, WorkMode } from "@/types";

const POLL_INTERVAL_MS = 15_000;

export function useAutopilotRunner(onConversationsChanged?: () => void) {
  useEffect(() => {
    let stopped = false;

    const runOne = async (run: QueuedAutopilotRun) => {
      const agent = getRuntimeAgentId(run.agent_name);
      if (!agent) {
        await autopilotApi.markRun(run.run_id, "failed").catch(() => {});
        toast.error(`Autopilot「${run.title}」失败：${run.agent_name} 未适配运行`);
        return;
      }
      try {
        const permission: RuntimePermissionPolicy =
          run.permission === "full_access"
            ? { kind: "full_access", confirmed: true }
            : { kind: run.permission as "ask_every_time" | "ask_on_risk" };
        const session = await runtimeApi.startSession({
          conversation_id: run.conversation_id,
          agent,
          workspace_path: run.workspace_path,
          model: { kind: "agent_default" },
          permission,
          work_mode: run.work_mode as WorkMode,
        });
        await runtimeApi.sendMessage(session.id, run.prompt, run.prompt);
        await autopilotApi.markRun(run.run_id, "done");
        toast.message(`Autopilot 已触发：${run.title}`, { description: "已创建会话并派给 Agent" });
        onConversationsChanged?.();
      } catch (error) {
        console.error("[autopilot] run failed", error);
        await autopilotApi.markRun(run.run_id, "failed").catch(() => {});
      }
    };

    const tick = async () => {
      if (stopped) return;
      try {
        const runs = await autopilotApi.takeQueuedRuns();
        for (const run of runs) {
          if (stopped) break;
          await runOne(run);
        }
      } catch {
        /* transient DB/poll error — try again next tick */
      }
    };

    void tick();
    const timer = window.setInterval(() => void tick(), POLL_INTERVAL_MS);
    return () => {
      stopped = true;
      window.clearInterval(timer);
    };
  }, [onConversationsChanged]);
}

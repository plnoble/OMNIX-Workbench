/**
 * useDiagnostics — Environment diagnostics and tool repair
 */

import { useState, useCallback } from "react";
import { diagnosticsApi } from "@/lib/tauri-api";
import { listen } from "@tauri-apps/api/event";

export interface UseDiagnosticsReturn {
  envDiagnostics: Record<string, string>;
  repairingTool: string;
  repairLogs: string;

  runDiagnostics: () => Promise<void>;
  repairTool: (toolName: string) => Promise<void>;
}

export function useDiagnostics(): UseDiagnosticsReturn {
  const [envDiagnostics, setEnvDiagnostics] = useState<Record<string, string>>({});
  const [repairingTool, setRepairingTool] = useState("");
  const [repairLogs, setRepairLogs] = useState("");

  const runDiagnostics = useCallback(async () => {
    try {
      const diag = await diagnosticsApi.run();
      setEnvDiagnostics(diag);
    } catch (e) {
      console.error("[useDiagnostics] Failed to run diagnostics:", e);
    }
  }, []);

  const repairTool = useCallback(async (toolName: string) => {
    setRepairingTool(toolName);
    setRepairLogs(`--- 开始修复 ${toolName} 环境... ---\n`);

    const unlistenLog = listen<{ log: string }>("omnix-repair-log", (event) => {
      setRepairLogs((prev) => prev + event.payload.log);
    });

    try {
      await diagnosticsApi.repair(toolName);
      await runDiagnostics();
    } catch (e) {
      console.error("[useDiagnostics] Repair failed:", e);
      throw e;
    } finally {
      (await unlistenLog)();
      setRepairingTool("");
    }
  }, [runDiagnostics]);

  return {
    envDiagnostics, repairingTool, repairLogs,
    runDiagnostics, repairTool,
  };
}

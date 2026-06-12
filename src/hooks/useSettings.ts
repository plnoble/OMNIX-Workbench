/**
 * useSettings — Application settings state and persistence
 *
 * Manages: targetModel, gpuAcceleration,
 * idleTimeout, autoStart, startToTray, useWsl, wslDistro, gatewayStatus,
 * themeMode
 *
 * Note: apiKey, apiHost, and proxyPort are NO LONGER user-configurable here.
 *   - API keys and base URLs are managed per-platform in the Model Hub.
 *   - The proxy port is an internal constant (1421), not a user setting.
 */

import { useState, useCallback, useRef } from "react";
import { settingsApi } from "@/lib/tauri-api";
import type { GatewayStatus } from "@/types";
import { DEFAULT_IDLE_TIMEOUT, DEFAULT_WSL_DISTRO } from "@/lib/constants";

export type ThemeMode = "dark" | "light" | "auto";

interface SettingsState {
  targetModel: string;
  gpuAcceleration: boolean;
  idleTimeout: string;
  autoStart: boolean;
  startToTray: boolean;
  useWsl: boolean;
  wslDistro: string;
  gatewayStatus: GatewayStatus;
  themeMode: ThemeMode;
}

interface SettingsActions {
  setTargetModel: (v: string) => void;
  setGpuAcceleration: (v: boolean) => void;
  setIdleTimeout: (v: string) => void;
  setAutoStart: (v: boolean) => void;
  setStartToTray: (v: boolean) => void;
  setUseWsl: (v: boolean) => void;
  setWslDistro: (v: string) => void;
  setGatewayStatus: (v: GatewayStatus) => void;
  setThemeMode: (v: ThemeMode) => void;
  loadSettings: () => Promise<void>;
  saveSettings: () => Promise<void>;
}

export type UseSettingsReturn = SettingsState & SettingsActions;

export function useSettings(): UseSettingsReturn {
  const gatewayTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const [targetModel, setTargetModel] = useState("");
  const [gpuAcceleration, setGpuAcceleration] = useState(true);
  const [idleTimeout, setIdleTimeout] = useState(DEFAULT_IDLE_TIMEOUT);
  const [autoStart, setAutoStart] = useState(false);
  const [startToTray, setStartToTray] = useState(true);
  const [useWsl, setUseWsl] = useState(false);
  const [wslDistro, setWslDistro] = useState(DEFAULT_WSL_DISTRO);
  const [gatewayStatus, setGatewayStatus] = useState<GatewayStatus>("idle");
  const [themeMode, setThemeMode] = useState<ThemeMode>("auto");

  const loadSettings = useCallback(async () => {
    try {
      const [model, gpu, timeout, start, tray, theme] = await Promise.all([
        settingsApi.get("target_model"),
        settingsApi.get("gpu_acceleration"),
        settingsApi.get("idle_timeout_min"),
        settingsApi.get("auto_start"),
        settingsApi.get("start_to_tray"),
        settingsApi.get("theme_mode"),
      ]);

      if (model) setTargetModel(model);
      if (gpu) setGpuAcceleration(gpu === "true");
      if (timeout) setIdleTimeout(timeout);
      if (start) setAutoStart(start === "true");
      if (tray) setStartToTray(tray === "true");
      // Theme migration: old default was "dark", new default is "auto"
      // On first run after upgrade, migrate DB-stored "dark" → "auto" once
      const migrated = await settingsApi.get("theme_mode_migrated");
      if (!migrated) {
        // No migration flag — if DB had old default "dark", change to "auto"
        if (theme === "dark" || !theme) {
          setThemeMode("auto");
          await settingsApi.set("theme_mode", "auto");
        } else if (theme === "light" || theme === "auto") {
          setThemeMode(theme);
        }
        await settingsApi.set("theme_mode_migrated", "true");
      } else {
        // Already migrated — respect DB value
        if (theme === "dark" || theme === "light" || theme === "auto") setThemeMode(theme);
      }
    } catch (e) {
      console.error("[useSettings] Failed to load settings:", e);
    }
  }, []);

  const saveSettings = useCallback(async () => {
    setGatewayStatus("busy");
    try {
      await Promise.all([
        settingsApi.set("target_model", targetModel),
        settingsApi.set("gpu_acceleration", gpuAcceleration ? "true" : "false"),
        settingsApi.set("idle_timeout_min", idleTimeout),
        settingsApi.set("auto_start", autoStart ? "true" : "false"),
        settingsApi.set("start_to_tray", startToTray ? "true" : "false"),
        settingsApi.set("theme_mode", themeMode),
      ]);

      await settingsApi.syncExternalConfigs();

      // Clear any pending gateway status update before scheduling a new one
      if (gatewayTimerRef.current) clearTimeout(gatewayTimerRef.current);
      gatewayTimerRef.current = setTimeout(() => {
        setGatewayStatus("idle");
      }, 500);
    } catch (e) {
      console.error("[useSettings] Failed to save settings:", e);
      setGatewayStatus("error");
      throw e; // Re-throw so caller can show feedback
    }
  }, [targetModel, gpuAcceleration, idleTimeout, autoStart, startToTray, themeMode]);

  return {
    targetModel, setTargetModel,
    gpuAcceleration, setGpuAcceleration,
    idleTimeout, setIdleTimeout,
    autoStart, setAutoStart,
    startToTray, setStartToTray,
    useWsl, setUseWsl,
    wslDistro, setWslDistro,
    gatewayStatus, setGatewayStatus,
    themeMode, setThemeMode,
    loadSettings,
    saveSettings,
  };
}

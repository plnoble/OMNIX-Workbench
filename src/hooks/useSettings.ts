/**
 * useSettings — Application settings state and persistence
 *
 * Manages: apiKey, apiHost, targetModel, proxyPort, gpuAcceleration,
 * idleTimeout, autoStart, startToTray, useWsl, wslDistro, gatewayStatus,
 * themeMode
 */

import { useState, useCallback } from "react";
import { settingsApi } from "@/lib/tauri-api";
import type { GatewayStatus } from "@/types";
import { DEFAULT_PROXY_PORT, DEFAULT_IDLE_TIMEOUT, DEFAULT_WSL_DISTRO } from "@/lib/constants";

export type ThemeMode = "dark" | "light" | "auto";

interface SettingsState {
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
  gatewayStatus: GatewayStatus;
  themeMode: ThemeMode;
}

interface SettingsActions {
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
  setGatewayStatus: (v: GatewayStatus) => void;
  setThemeMode: (v: ThemeMode) => void;
  loadSettings: () => Promise<void>;
  saveSettings: () => Promise<void>;
}

export type UseSettingsReturn = SettingsState & SettingsActions;

export function useSettings(): UseSettingsReturn {
  const [apiKey, setApiKey] = useState("");
  const [apiHost, setApiHost] = useState("");
  const [targetModel, setTargetModel] = useState("");
  const [proxyPort, setProxyPort] = useState(DEFAULT_PROXY_PORT);
  const [gpuAcceleration, setGpuAcceleration] = useState(true);
  const [idleTimeout, setIdleTimeout] = useState(DEFAULT_IDLE_TIMEOUT);
  const [autoStart, setAutoStart] = useState(false);
  const [startToTray, setStartToTray] = useState(true);
  const [useWsl, setUseWsl] = useState(false);
  const [wslDistro, setWslDistro] = useState(DEFAULT_WSL_DISTRO);
  const [gatewayStatus, setGatewayStatus] = useState<GatewayStatus>("idle");
  const [themeMode, setThemeMode] = useState<ThemeMode>("dark");

  const loadSettings = useCallback(async () => {
    try {
      const [key, host, model, port, gpu, timeout, start, tray, theme] = await Promise.all([
        settingsApi.get("api_key"),
        settingsApi.get("api_host"),
        settingsApi.get("target_model"),
        settingsApi.get("proxy_port"),
        settingsApi.get("gpu_acceleration"),
        settingsApi.get("idle_timeout_min"),
        settingsApi.get("auto_start"),
        settingsApi.get("start_to_tray"),
        settingsApi.get("theme_mode"),
      ]);

      if (key) setApiKey(key);
      if (host) setApiHost(host);
      if (model) setTargetModel(model);
      if (port) setProxyPort(port);
      if (gpu) setGpuAcceleration(gpu === "true");
      if (timeout) setIdleTimeout(timeout);
      if (start) setAutoStart(start === "true");
      if (tray) setStartToTray(tray === "true");
      if (theme === "dark" || theme === "light" || theme === "auto") setThemeMode(theme);
    } catch (e) {
      console.error("[useSettings] Failed to load settings:", e);
    }
  }, []);

  const saveSettings = useCallback(async () => {
    setGatewayStatus("busy");
    try {
      await Promise.all([
        settingsApi.set("api_key", apiKey),
        settingsApi.set("api_host", apiHost),
        settingsApi.set("target_model", targetModel),
        settingsApi.set("proxy_port", proxyPort),
        settingsApi.set("gpu_acceleration", gpuAcceleration ? "true" : "false"),
        settingsApi.set("idle_timeout_min", idleTimeout),
        settingsApi.set("auto_start", autoStart ? "true" : "false"),
        settingsApi.set("start_to_tray", startToTray ? "true" : "false"),
        settingsApi.set("theme_mode", themeMode),
      ]);

      await settingsApi.syncExternalConfigs();

      setTimeout(() => {
        setGatewayStatus(apiKey.trim().length > 0 ? "idle" : "error");
      }, 500);
    } catch (e) {
      console.error("[useSettings] Failed to save settings:", e);
      setGatewayStatus("error");
      throw e; // Re-throw so caller can show feedback
    }
  }, [apiKey, apiHost, targetModel, proxyPort, gpuAcceleration, idleTimeout, autoStart, startToTray, themeMode]);

  return {
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
    gatewayStatus, setGatewayStatus,
    themeMode, setThemeMode,
    loadSettings,
    saveSettings,
  };
}

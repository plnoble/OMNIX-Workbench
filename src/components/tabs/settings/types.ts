/** Split from SettingsTab.tsx — pure move, no behavior change. */
import type { BackupTableInfo, ImportResult, McpServer, ModelPlatform, ModelTestState, PlatformModel, SelectionHistoryEntry, SettingsSubTab } from "@/types";

export interface SettingsTabProps {
  settingsSubTab: SettingsSubTab;
  setSettingsSubTab: (tab: SettingsSubTab) => void;

  // Diagnostics sub-tab — 整块诊断/概览面板由 App 构造后作为节点传入（合并原顶栏"诊断"入口）。
  // 可选：专页复用组件（如 McpTab）不提供，它们也不会渲染诊断子页。
  diagnosticsPanel?: React.ReactNode;

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
  // onToggleCapability removed — capabilities are now auto-detected
  onTestModel: (id: string) => Promise<import("@/types").HealthCheckDetail>;
  onDeleteModel: (id: string) => void;
  batchTesting: Record<string, boolean>;
  onBatchTestModels: (platformId: string) => void;

  // 账号凭据（agent_accounts）不在设置里——它的唯一入口是「智能体」页的
  // 右侧详情，那里按 agent 过滤，看得出哪个账号属于谁。

  // Settings form
  targetModel: string;
  gpuAcceleration: boolean;
  idleTimeout: string;
  autoStart: boolean;
  startToTray: boolean;
  useWsl: boolean;
  wslDistro: string;
  setTargetModel: (v: string) => void;
  setGpuAcceleration: (v: boolean) => void;
  setIdleTimeout: (v: string) => void;
  setAutoStart: (v: boolean) => void;
  setStartToTray: (v: boolean) => void;
  setUseWsl: (v: boolean) => void;
  setWslDistro: (v: string) => void;
  onSaveSettings: () => Promise<void>;

  // Selection Assistant
  selectionCaptureMode: string;
  selectionShowOnCapture: boolean;
  selectionAutoCaptureEnabled: boolean;
  selectionPreserveClipboard: boolean;
  isSelectionCapturing: boolean;
  lastSelectionCapture: string | null;
  selectionCaptureError: string | null;
  selectionHistory: SelectionHistoryEntry[];
  onSetSelectionCaptureMode: (v: string) => void;
  onSetSelectionShowOnCapture: (v: boolean) => void;
  onSetSelectionAutoCaptureEnabled: (v: boolean) => void;
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
  onReloadMcpServers?: () => Promise<void>;

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



export type PlatformSubTabProps = Pick<
  SettingsTabProps,
  | "platforms"
  | "selectedPlatformId"
  | "platformModels"
  | "modelTestingState"
  | "fetchingModels"
  | "onSelectPlatform"
  | "onTogglePlatform"
  | "onAddPlatform"
  | "onEditPlatform"
  | "onDeletePlatform"
  | "onFetchRemoteModels"
  | "onAddModel"
  | "onToggleModelEnabled"
  | "onTestModel"
  | "onDeleteModel"
  | "batchTesting"
  | "onBatchTestModels"
>;


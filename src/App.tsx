/**
 * OMNIX Workbench - Main Application Orchestrator
 *
 * This file is the thin orchestration layer that:
 * 1. Instantiates all custom hooks
 * 2. Manages top-level UI state (activeTab, showTour)
 * 3. Wires hook state/actions into child components
 * 4. Renders the layout skeleton
 *
 * All business logic lives in hooks. All rendering lives in components.
 */

import { useState, useEffect, useRef, Suspense, lazy, useMemo, type ComponentType } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
// Global shortcuts registered on Rust side (lib.rs) for reliability

// Hooks
import { useTheme } from "@/hooks/useTheme";
import { useNavigationLayout } from "@/hooks/useNavigationLayout";
import {
  AppStoreProvider,
  useAccountsStore,
  useBackupStore,
  useConversationsStore,
  useCronStore,
  useDiagnosticsStore,
  useMcpServersStore,
  usePlatformsStore,
  usePreviewStore,
  useRemoteAccessStore,
  useSearchStore,
  useSelectionStore,
  useSettingsStore,
  useTranslationStore,
} from "@/store/AppStore";

// Layout (eager — always visible)
import { AppSidebar } from "@/components/layout/AppSidebar";
import { AppHeader } from "@/components/layout/AppHeader";
import { PreviewPane } from "@/components/layout/PreviewPane";
import { CommandPalette } from "@/components/CommandPalette";
import { ConversationHistoryView } from "@/components/ConversationHistoryView";

// Modals (eager — lightweight dialogs)
import { PlatformModal } from "@/components/modals/PlatformModal";
import { ModelModal } from "@/components/modals/ModelModal";
import { AccountModal } from "@/components/modals/AccountModal";
import { CronModal } from "@/components/modals/CronModal";
import { WorkspaceModal } from "@/components/modals/WorkspaceModal";

// Toast
import { Toaster, toast } from "@/components/ui/sonner";
import { UpdateManager } from "@/components/UpdateManager";

// Types
import type { SettingsSubTab } from "@/types";
import { APP_ENTRIES } from "@/lib/appRegistry";
import { evolutionApi, projectProtocolApi } from "@/lib/tauri-api";

// ── Lazy-loaded tabs (code-split per route) ──────────
const StatusDock = lazy(() => import("./StatusDock"));
const SpeakerView = lazy(() => import("./SpeakerView"));
const QuickAssistant = lazy(() => import("./QuickAssistant").then(m => ({ default: m.QuickAssistant })));
const DashboardTab = lazy(() => import("@/components/tabs/DashboardTab").then(m => ({ default: m.DashboardTab })));
const ChatTab = lazy(() => import("@/components/tabs/ChatTab").then(m => ({ default: m.ChatTab })));
const AgentHubTab = lazy(() => import("@/components/tabs/AgentHubTab").then(m => ({ default: m.AgentHubTab })));
const CompareTab = lazy(() => import("@/components/tabs/CompareTab").then(m => ({ default: m.CompareTab })));
const TeamTab = lazy(() => import("@/components/tabs/TeamTab").then(m => ({ default: m.TeamTab })));
const MemoryTab = lazy(() => import("@/components/tabs/MemoryTab").then(m => ({ default: m.MemoryTab })));
const SkillTab = lazy(() => import("@/components/tabs/SkillTab").then(m => ({ default: m.SkillTab })));
const KnowledgeTab = lazy(() => import("@/components/tabs/KnowledgeTab").then(m => ({ default: m.KnowledgeTab })));
const CronTab = lazy(() => import("@/components/tabs/CronTab").then(m => ({ default: m.CronTab })));
const ModelsTab = lazy(() => import("@/components/tabs/ModelsTab").then(m => ({ default: m.ModelsTab })));
const McpTab = lazy(() => import("@/components/tabs/McpTab").then(m => ({ default: m.McpTab })));
const HooksTab = lazy(() => import("@/components/tabs/HooksTab").then(m => ({ default: m.HooksTab })));
const NotesTab = lazy(() => import("@/components/tabs/NotesTab").then(m => ({ default: m.NotesTab })));
const TranslateTab = lazy(() => import("@/components/tabs/TranslateTab").then(m => ({ default: m.TranslateTab })));
const StudioTab = lazy(() => import("@/components/tabs/StudioTab").then(m => ({ default: m.StudioTab })));
const AutopilotsTab = lazy(() => import("@/components/tabs/AutopilotsTab").then(m => ({ default: m.AutopilotsTab })));
const OfficeTab = lazy(() => import("@/components/tabs/OfficeTab").then(m => ({ default: m.OfficeTab })));
const MonitorTab = lazy(() => import("@/components/tabs/MonitorTab").then(m => ({ default: m.MonitorTab })));
const AuthCenterTab = lazy(() => import("@/components/tabs/AuthCenterTab").then(m => ({ default: m.AuthCenterTab })));
const LocalModelPickerTab = lazy(() => import("@/components/tabs/LocalModelPickerTab").then(m => ({ default: m.LocalModelPickerTab })));
const SearchResourceTab = lazy(() => import("@/components/tabs/SearchResourceTab").then(m => ({ default: m.SearchResourceTab })));
const QuickAssistantTab = lazy(() => import("@/components/tabs/QuickAssistantTab").then(m => ({ default: m.QuickAssistantTab })));
const AssistantsTab = lazy(() => import("@/components/tabs/AssistantsTab").then(m => ({ default: m.AssistantsTab })));
const SettingsTab = lazy(() => import("@/components/tabs/SettingsTab").then(m => ({ default: m.SettingsTab })));
const WelcomeTour = lazy(() => import("./WelcomeTour").then(m => ({ default: m.WelcomeTour })));
const RemoteDevTab = lazy(() => import("@/components/tabs/RemoteDevTab").then(m => ({ default: m.RemoteDevTab })));

// ── Prop-less tab registry ───────────────────────────
// Pages that take no props render via one lookup instead of a conditional
// per tab — adding such a page is a single entry here (+ appRegistry.tsx).
const SIMPLE_TABS: Record<string, ComponentType> = {
  compare: CompareTab,
  hooks: HooksTab,
  notes: NotesTab,
  translate: TranslateTab,
  studio: StudioTab,
  autopilot: AutopilotsTab,
  // Office 工作台（合并项）：旧的 slides/write/excel id 保留为别名，
  // 历史导航状态/快捷入口落到同一个工作台，不断链。
  office: OfficeTab,
  slides: OfficeTab,
  write: OfficeTab,
  excel: OfficeTab,
  // 监控中心（合并项）：总控/用量/画像收进一个入口，旧 id 别名。
  supervision: MonitorTab,
  usage: MonitorTab,
  profile: MonitorTab,
  "auth-center": AuthCenterTab,
  "local-models": LocalModelPickerTab,
  "remote-dev": RemoteDevTab,
  memories: MemoryTab,
  skills: SkillTab,
  knowledge: KnowledgeTab,
};

// ── Suspense fallback ────────────────────────────────
function LazyFallback() {
  return (
    <div className="flex items-center justify-center h-full text-muted-foreground text-sm animate-pulse">
      加载中…
    </div>
  );
}

// ── Root App ────────────────────────────────────────

function App() {
  const urlParams = new URLSearchParams(window.location.search);
  const isStatusDock = urlParams.get("window") === "status-dock";
  const isQuickAssistant = urlParams.get("window") === "quick-assistant";

  if (isQuickAssistant) {
    return (
      <Suspense fallback={<LazyFallback />}>
        <QuickAssistant />
      </Suspense>
    );
  }

  if (isStatusDock) {
    return (
      <Suspense fallback={<LazyFallback />}>
        <StatusDock />
      </Suspense>
    );
  }

  // 演讲者视图：第二块屏上的独立窗口，备注只给讲的人看
  if (urlParams.get("window") === "slides-speaker") {
    return (
      <Suspense fallback={<LazyFallback />}>
        <SpeakerView />
      </Suspense>
    );
  }

  return <MainApp />;
}

// ── Main Orchestrator ────────────────────────────────

function MainApp() {
  // hook 的实例化搬到了 AppStoreProvider——它们互相有依赖，必须在同一处按序运行。
  return (
    <AppStoreProvider>
      <MainAppShell />
    </AppStoreProvider>
  );
}

function MainAppShell() {
  // 同一批对象，来源从「上游传进来」换成「从 store 取」。
  const settings = useSettingsStore();
  const platforms = usePlatformsStore();
  const accounts = useAccountsStore();
  const convs = useConversationsStore();
  const cron = useCronStore();
  const preview = usePreviewStore();
  const diagnostics = useDiagnosticsStore();
  const remote = useRemoteAccessStore();
  const selection = useSelectionStore();
  const translation = useTranslationStore();
  const search = useSearchStore();
  const mcpServers = useMcpServersStore();
  const backup = useBackupStore();
  const navigation = useNavigationLayout();

  // ── Apply theme ──────────────────────────────────────
  useTheme(settings.themeMode);

  // ── Top-level UI state ────────────────────────────
  const [activeTab, setActiveTab] = useState("chat");
  const [showTour, setShowTour] = useState(false);
  const [settingsSubTab, setSettingsSubTab] = useState<SettingsSubTab>("system");
  const [showCommandPalette, setShowCommandPalette] = useState(false);
  const [showHistoryFullscreen, setShowHistoryFullscreen] = useState(false);
  // Archive intent: both the sidebar and history view request archive here, and
  // this shared dialog lets the user distill-then-archive or just archive.
  const [pendingArchive, setPendingArchive] = useState<{ id: string; title: string } | null>(null);
  const [archiving, setArchiving] = useState(false);

  // ── Initialization ────────────────────────────────

  useEffect(() => {
    settings.loadSettings();
    platforms.loadPlatforms();
    convs.loadConversations();
    convs.detectAgents();
    accounts.loadAccounts();
    platforms.loadActiveModels();
    selection.loadSelectionSettings();
    // 不加这一句，设置里的翻译项永远显示默认值——存进去的翻译模型读不回来。
    void translation.loadTranslationSettings();
    navigation.loadLayout();
    checkOnboarding();
  }, []); // eslint-disable-line react-hooks/exhaustive-deps -- mount-only init: all load functions are stable ref-less fetchers

  // Warm only the handful of chunks the user is most likely to open next, so
  // switching to a core surface doesn't flash a loader. Warming all ~18 lazy
  // tabs on idle defeated the point of code-splitting (it pulled almost the
  // whole app anyway); the rest load on demand.
  useEffect(() => {
    const warm = () => {
      void import("@/components/tabs/ChatTab"); // chat + work surfaces
      void import("@/components/tabs/TeamTab");
      void import("@/components/tabs/SettingsTab");
    };
    const ric = (window as unknown as { requestIdleCallback?: (cb: () => void) => number }).requestIdleCallback;
    const id = ric ? ric(warm) : window.setTimeout(warm, 1500);
    return () => {
      const cancel = (window as unknown as { cancelIdleCallback?: (id: number) => void }).cancelIdleCallback;
      if (cancel) cancel(id); else clearTimeout(id);
    };
  }, []);

  const checkOnboarding = async () => {
    try {
      const state = await invoke<string | null>("get_app_setting", { key: "onboarding_completed" });
      if (state !== "true") {
        setShowTour(true);
      }
    } catch (e) {
      console.error("Failed to check onboarding state:", e);
    }
  };

  // ── Listen for StatusDock navigation events ────────
  // (Listener registered below, after handleTabChange is defined, via a ref so
  //  it always calls the latest handler without re-subscribing on every render.)

  // ── Command Palette shortcut (Ctrl+K) ──────────────

  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key === "k") {
        e.preventDefault();
        setShowCommandPalette((prev) => !prev);
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, []);

  // ── Selection Assistant & Quick Assistant shortcuts ──
  // (Registered on Rust side in lib.rs setup for reliability — no frontend registration needed)

  // ── Tab change handler ────────────────────────────

  const normalizeTab = (tab: string) => {
    if (tab === "workbench") return "work";
    if (tab === "memory") return "memories";
    return tab;
  };

  const handleTabChange = (tab: string) => {
    let nextTab = normalizeTab(tab);
    // 诊断已并入设置：任何跳"dashboard"的入口都落到 设置›诊断 子页。
    if (nextTab === "dashboard") {
      setSettingsSubTab("diagnostics");
      nextTab = "settings";
    }
    setActiveTab(nextTab);
    if (nextTab === "chat" || nextTab === "work") {
      convs.enterSurface(nextTab);
    }
    if (nextTab === "cron") {
      cron.loadCronTasks();
      cron.loadCronRuns();
    }
    if (nextTab === "models") {
      platforms.loadPlatforms();
      platforms.loadActiveModels();
    }
    if (nextTab === "mcp") {
      mcpServers.loadMcpServers();
    }
    if (nextTab === "search") {
      search.loadProviders();
    }
    if (nextTab === "settings") {
      platforms.loadPlatforms();
      accounts.loadAccounts();
      search.loadProviders();
      mcpServers.loadMcpServers();
      backup.loadBackupInfo();
    }
  };

  // Keep a ref to the latest handler so the navigation listener (registered once)
  // never calls a stale closure of handleTabChange.
  const handleTabChangeRef = useRef(handleTabChange);
  handleTabChangeRef.current = handleTabChange;

  useEffect(() => {
    const unlisten = listen("omnix-navigate-settings", () => {
      handleTabChangeRef.current("settings");
    });
    return () => { unlisten.then((fn) => fn()); };
  }, []);

  // `omnix-notification` 的**落地端**。此前它有两个发送方却没有任何监听方：
  // `hooks.rs` 的 notify 动作（钩子面板里「桌面通知」还是默认动作，跑完还会往
  // 运行记录里写「已发送通知」）和 `send_desktop_notification` 命令。也就是说
  // 用户配了通知钩子、看见「已发送通知」，屏幕上什么都不会出现。
  //
  // 这里用应用内 toast 落地——OMNIX 没装 tauri-plugin-notification，为这个加一个
  // 系统级依赖不值当；真正缺的是「让它出现」。
  useEffect(() => {
    const unlisten = listen<{ title?: string; body?: string }>("omnix-notification", (event) => {
      const { title, body } = event.payload ?? {};
      const text = title?.trim() || "通知";
      if (body?.trim()) {
        toast.message(text, { description: body });
      } else {
        toast.message(text);
      }
    });
    return () => { unlisten.then((fn) => fn()); };
  }, []);

  // ── Save handlers with user feedback ──────────────

  const handleSavePlatform = async () => {
    try {
      await platforms.savePlatform();
      toast.success("平台配置保存成功！");
    } catch (e) {
      toast.error("保存失败：" + e);
    }
  };

  const handleSaveCustomModel = async () => {
    try {
      await platforms.saveCustomModel();
      toast.success("自定义模型保存成功！");
    } catch (e) {
      toast.error("保存失败：" + e);
    }
  };

  const handleSaveAccount = async () => {
    try {
      await accounts.saveAccount();
      toast.success("账户凭证保存成功！");
    } catch (e) {
      toast.error("保存失败：" + e);
    }
  };

  const handleSaveCronTask = async () => {
    try {
      await cron.saveCronTask();
      toast.success("计划任务配置保存成功！");
    } catch (e) {
      toast.error("保存计划任务失败：" + e);
    }
  };

  const handleSaveWorkspaceChat = async (options?: { enableProjectProtocol: boolean }) => {
    try {
      if (options?.enableProjectProtocol) {
        await projectProtocolApi.initWorkspace(convs.workspaceFormPath, undefined, true);
        toast.success("项目协议已初始化");
        // Pre-cache this workspace's relevance profile (best-effort, non-blocking)
        // so injected experience is ranked by how well it fits this project's stack.
        evolutionApi.refreshWorkspace(convs.workspaceFormPath).catch(() => {});
      }
      await convs.saveWorkspaceChat();
    } catch (e) {
      toast.error("新建项目会话失败：" + e);
    }
  };

  // ── Derived state ─────────────────────────────────

  const entriesById = useMemo(() => new Map(APP_ENTRIES.map((entry) => [entry.id, entry])), []);
  const pinnedEntries = useMemo(
    () => navigation.layout.pinned.map((id) => entriesById.get(id)).filter(Boolean) as typeof APP_ENTRIES,
    [entriesById, navigation.layout.pinned]
  );
  const launcherEntries = useMemo(
    () => navigation.layout.launcher.map((id) => entriesById.get(id)).filter(Boolean) as typeof APP_ENTRIES,
    [entriesById, navigation.layout.launcher]
  );
  const hiddenEntries = useMemo(
    () => navigation.layout.hidden.map((id) => entriesById.get(id)).filter(Boolean) as typeof APP_ENTRIES,
    [entriesById, navigation.layout.hidden]
  );

  const showConversations = activeTab === "chat" || activeTab === "work" || activeTab === "team";
  const SimpleTabComponent = SIMPLE_TABS[activeTab];
  const showPreviewButton = !!(convs.chatWorkspace && convs.chatWorkspace !== "direct");

  // ── Render ────────────────────────────────────────

  return (
    <div className="flex h-screen w-screen flex-col overflow-hidden">
      <AppHeader
        activeTab={activeTab}
        activeAgent={convs.activeAgent}
        chatWorkspace={convs.chatWorkspace}
        gatewayStatus={settings.gatewayStatus}
        pinnedEntries={pinnedEntries}
        launcherEntries={launcherEntries}
        hiddenEntries={hiddenEntries}
        themeMode={settings.themeMode}
        showPreviewButton={showPreviewButton}
        isPreviewOpen={preview.showPreviewPane}
        onNavigate={handleTabChange}
        onMoveEntry={async (id, placement) => {
          try {
            await navigation.moveEntry(id, placement);
          } catch (error) {
            toast.error("保存导航布局失败：" + error);
          }
        }}
        onReorderEntry={async (id, direction) => {
          try {
            await navigation.reorderEntry(id, direction);
          } catch (error) {
            toast.error("保存导航顺序失败：" + error);
          }
        }}
        onResetNavigation={async () => {
          await navigation.resetLayout();
          toast.success("已恢复默认导航布局");
        }}
        onToggleTheme={() => {
          const modes: ("dark" | "light" | "auto")[] = ["dark", "light", "auto"];
          const current = modes.indexOf(settings.themeMode);
          settings.setThemeMode(modes[(current + 1) % modes.length]);
        }}
        onTogglePreview={() => {
          preview.setShowPreviewPane(!preview.showPreviewPane);
          if (!preview.showPreviewPane) preview.loadPreviewFiles();
        }}
      />

      <div className="flex min-h-0 flex-1 overflow-hidden">
      <AppSidebar
        activeTab={activeTab}
        onTabChange={handleTabChange}
        gatewayStatus={settings.gatewayStatus}
        showConversations={showConversations}
        conversations={convs.conversations}
        activeAgent={convs.activeAgent}
        currentConvId={convs.currentConvId}
        activeSessions={convs.activeSessions}
        onSelectConversation={convs.selectConversation}
        onDeleteConversation={convs.deleteConversation}
        onArchiveConversation={(id, title) => setPendingArchive({ id, title })}
        onOpenHistoryFullscreen={() => setShowHistoryFullscreen(true)}
        onNewConversation={convs.newConversation}
        onOpenWorkspaceModal={() => convs.setIsWorkspaceModalOpen(true)}
      />

      <main className="relative flex min-w-0 flex-1 flex-col bg-background">
        <div className="flex flex-1 min-w-0 overflow-hidden">
          <Suspense fallback={<LazyFallback />}>
            {(activeTab === "chat" || activeTab === "work") && (
              <ChatTab
                surface={activeTab}
                onSuggestTeam={(prompt) => {
                  if (prompt.trim()) convs.setCollabStdin(prompt);
                  handleTabChange("team");
                  toast.message("已切到团队入口，队长计划仍需你确认后才会启动 Worker。");
                }}
              />
            )}

            {activeTab === "agents" && (
              <AgentHubTab
                onStartWork={(name) => {
                  convs.selectAgent(name);
                  handleTabChange("work");
                }}
                onOpenAuthCenter={() => setActiveTab("auth-center")}
              />
            )}

            {SimpleTabComponent && <SimpleTabComponent />}
            {activeTab === "models" && (
              <ModelsTab />
            )}
            {activeTab === "search" && (
              <SearchResourceTab />
            )}
            {activeTab === "quick-assistant" && (
              <QuickAssistantTab />
            )}
            {activeTab === "assistants" && (
              <AssistantsTab
                onUseTemplate={(template) => {
                  convs.setChatInput(template.instructions);
                  handleTabChange("work");
                  toast.success(`已带入助手：${template.name}`);
                }}
              />
            )}
            {activeTab === "mcp" && (
              <McpTab />
            )}

            {activeTab === "team" && (
              <TeamTab />
            )}

            {activeTab === "cron" && (
              <CronTab />
            )}

            {activeTab === "settings" && (
              <SettingsTab
                settingsSubTab={settingsSubTab}
                setSettingsSubTab={setSettingsSubTab}
                diagnosticsPanel={
                  <DashboardTab
                    activeSessionsCount={convs.activeSessions.length}
                    detectedAgents={convs.detectedAgents}
                    envDiagnostics={diagnostics.envDiagnostics}
                    repairLogs={diagnostics.repairLogs}
                    repairingTool={diagnostics.repairingTool}
                    remoteInfo={remote.remoteInfo}
                    onRunDiagnostics={diagnostics.runDiagnostics}
                    onRepairTool={diagnostics.repairTool}
                    onLoadRemoteAccess={remote.loadRemoteAccess}
                  />
                }
              />
            )}
          </Suspense>
        </div>
      </main>

      {/* Preview Pane */}
      {preview.showPreviewPane && showPreviewButton && (
        <PreviewPane
          previewFiles={preview.previewFiles}
          selectedPreviewFile={preview.selectedPreviewFile}
          previewType={preview.previewType}
          previewHtmlUrl={preview.previewHtmlUrl}
          previewTextContent={preview.previewTextContent}
          previewImageBase64={preview.previewImageBase64}
          chatWorkspace={convs.chatWorkspace}
          onSelectFile={preview.selectPreviewFile}
          onRefreshFiles={preview.loadPreviewFiles}
          onLoadGitDiff={preview.loadGitDiff}
          onClose={() => preview.setShowPreviewPane(false)}
        />
      )}
      </div>

      {/* Welcome Tour */}
      {showTour && (
        <Suspense fallback={null}>
          <WelcomeTour
            activeTab={activeTab}
            setActiveTab={handleTabChange}
            onClose={async () => {
              setShowTour(false);
              try {
                await invoke("set_app_setting", { key: "onboarding_completed", value: "true" });
              } catch (e) {
                console.error("Failed to save onboarding state:", e);
              }
            }}
          />
        </Suspense>
      )}

      {/* Modals */}
      <PlatformModal
        open={platforms.showPlatformModal}
        onOpenChange={(open) => { if (!open) platforms.closePlatformModal(); }}
        editingPlatform={platforms.editingPlatform}
        platformForm={platforms.platformForm}
        onFormChange={platforms.updatePlatformForm}
        onSave={handleSavePlatform}
      />

      <ModelModal
        open={platforms.showModelModal}
        onOpenChange={(open) => { if (!open) platforms.closeModelModal(); }}
        modelForm={platforms.modelForm}
        onNameChange={(name) => platforms.updateModelForm("model_name", name)}
        onSave={handleSaveCustomModel}
      />

      <AccountModal
        open={accounts.isAccountModalOpen}
        onOpenChange={(open) => { if (!open) accounts.closeAccountModal(); }}
        accFormId={accounts.accFormId}
        accFormName={accounts.accFormName}
        accFormKey={accounts.accFormKey}
        accFormHost={accounts.accFormHost}
        accFormModel={accounts.accFormModel}
        activeModels={platforms.activeModels}
        onFieldChange={accounts.updateAccForm}
        onSave={handleSaveAccount}
      />

      <CronModal
        open={cron.showCronModal}
        onOpenChange={(open) => { if (!open) cron.closeCronModal(); }}
        editingCron={cron.editingCron}
        cronForm={cron.cronForm}
        detectedAgents={convs.detectedAgents}
        onFormChange={cron.updateCronForm}
        onSave={handleSaveCronTask}
      />

      <WorkspaceModal
        open={convs.isWorkspaceModalOpen}
        onOpenChange={(open) => { if (!open) convs.setIsWorkspaceModalOpen(false); }}
        workspaceFormPath={convs.workspaceFormPath}
        onPathChange={convs.setWorkspaceFormPath}
        onSave={handleSaveWorkspaceChat}
      />

      {/* Toast Container */}
      <Toaster />

      {/* In-app software updates (startup check + dialog + deferred pill) */}
      <UpdateManager />

      {/* Command Palette */}
      <CommandPalette
        open={showCommandPalette}
        onClose={() => setShowCommandPalette(false)}
        onNavigate={handleTabChange}
        onToggleTheme={() => {
          const modes: ("dark" | "light" | "auto")[] = ["dark", "light", "auto"];
          const current = modes.indexOf(settings.themeMode);
          settings.setThemeMode(modes[(current + 1) % modes.length]);
        }}
      />

      {/* Conversation History Fullscreen */}
      {showHistoryFullscreen && (
        <ConversationHistoryView
          conversations={convs.conversations}
          archivedConversations={convs.archivedConversations}
          currentConvId={convs.currentConvId}
          activeSessions={convs.activeSessions}
          onSelectConversation={convs.selectConversation}
          onDeleteConversation={convs.deleteConversation}
          onArchiveConversation={(id, title) => setPendingArchive({ id, title })}
          onUnarchiveConversation={convs.unarchiveConversation}
          onNewConversation={convs.newConversation}
          onLoadArchived={convs.loadArchivedConversations}
          onClose={() => setShowHistoryFullscreen(false)}
        />
      )}

      {/* Archive confirm — distill-then-archive, or archive only. */}
      {pendingArchive && (
        <div className="fixed inset-0 z-[1100] flex items-center justify-center bg-black/60 p-4 backdrop-blur-sm">
          <div className="w-full max-w-md rounded-md border border-border glass-surface p-5 shadow-xl">
            <h3 className="m-0 mb-2 text-base font-semibold text-foreground">归档会话</h3>
            <p className="mb-1 break-words text-sm text-muted-foreground line-clamp-3">"{pendingArchive.title}"</p>
            <p className="mb-4 text-xs leading-5 text-muted-foreground">
              可先把这次对话蒸馏成经验（进入进化中枢待审），再归档。没什么营养的对话直接归档即可。
            </p>
            <div className="flex justify-end gap-2">
              <button
                className="rounded-md border border-border bg-muted/10 px-3 py-1.5 text-sm text-foreground hover:bg-muted/30 disabled:opacity-50"
                disabled={archiving}
                onClick={() => setPendingArchive(null)}
              >
                取消
              </button>
              <button
                className="rounded-md border border-border px-3 py-1.5 text-sm text-foreground hover:bg-muted/30 disabled:opacity-50"
                disabled={archiving}
                onClick={async () => {
                  const target = pendingArchive;
                  setArchiving(true);
                  try { await convs.archiveConversation(target.id, false); }
                  finally { setArchiving(false); setPendingArchive(null); }
                }}
              >
                只归档
              </button>
              <button
                className="rounded-md bg-primary px-3 py-1.5 text-sm text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
                disabled={archiving}
                onClick={async () => {
                  const target = pendingArchive;
                  setArchiving(true);
                  try { await convs.archiveConversation(target.id, true); }
                  finally { setArchiving(false); setPendingArchive(null); }
                }}
              >
                {archiving ? "处理中…" : "蒸馏并归档"}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

export default App;

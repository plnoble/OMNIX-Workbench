/**
 * OMNIX DevFlow — Main Application Orchestrator
 *
 * This file is the thin orchestration layer that:
 * 1. Instantiates all custom hooks
 * 2. Manages top-level UI state (activeTab, showTour)
 * 3. Wires hook state/actions into child components
 * 4. Renders the layout skeleton
 *
 * All business logic lives in hooks. All rendering lives in components.
 */

import { useState, useEffect, Suspense, lazy } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
// Global shortcuts registered on Rust side (lib.rs) for reliability

// Hooks
import { useSettings } from "@/hooks/useSettings";
import { usePlatforms } from "@/hooks/usePlatforms";
import { useAccounts } from "@/hooks/useAccounts";
import { useConversations } from "@/hooks/useConversations";
import { useCron } from "@/hooks/useCron";
import { usePreview } from "@/hooks/usePreview";
import { useDiagnostics } from "@/hooks/useDiagnostics";
import { useRemoteAccess } from "@/hooks/useRemoteAccess";
import { useResizer } from "@/hooks/useResizer";
import { useSelection } from "@/hooks/useSelection";
import { useTranslation } from "@/hooks/useTranslation";
import { useTheme } from "@/hooks/useTheme";
import { useSearch } from "@/hooks/useSearch";
import { useMcpServers } from "@/hooks/useMcpServers";
import { useBackup } from "@/hooks/useBackup";

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

// Types
import type { SettingsSubTab } from "@/types";

// ── Lazy-loaded tabs (code-split per route) ──────────
const StatusDock = lazy(() => import("./StatusDock"));
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
const SettingsTab = lazy(() => import("@/components/tabs/SettingsTab").then(m => ({ default: m.SettingsTab })));
const WelcomeTour = lazy(() => import("./WelcomeTour").then(m => ({ default: m.WelcomeTour })));

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

  return <MainApp />;
}

// ── Main Orchestrator ────────────────────────────────

function MainApp() {
  // ── Instantiate all hooks ──────────────────────────
  const settings = useSettings();
  const platforms = usePlatforms();
  const accounts = useAccounts(platforms.activeModels);
  const convs = useConversations(settings.gatewayStatus);
  const cron = useCron(convs.detectedAgents);
  const preview = usePreview(convs.chatWorkspace);
  const diagnostics = useDiagnostics();
  const remote = useRemoteAccess();
  const resizer = useResizer();
  const selection = useSelection();
  const translation = useTranslation();
  const search = useSearch();
  const mcpServers = useMcpServers();
  const backup = useBackup();

  // ── Apply theme ──────────────────────────────────────
  useTheme(settings.themeMode);

  // ── Top-level UI state ────────────────────────────
  const [activeTab, setActiveTab] = useState("dashboard");
  const [tipIndex, setTipIndex] = useState(0);
  const [showTour, setShowTour] = useState(false);
  const [settingsSubTab, setSettingsSubTab] = useState<SettingsSubTab>("platform");
  const [showCommandPalette, setShowCommandPalette] = useState(false);
  const [showHistoryFullscreen, setShowHistoryFullscreen] = useState(false);

  // ── Initialization ────────────────────────────────

  useEffect(() => {
    settings.loadSettings();
    platforms.loadPlatforms();
    convs.loadConversations();
    convs.detectAgents();
    accounts.loadAccounts();
    platforms.loadActiveModels();
    selection.loadSelectionSettings();
    checkOnboarding();
    setTipIndex(Math.floor(Math.random() * 5));
  }, []); // eslint-disable-line react-hooks/exhaustive-deps -- mount-only init: all load functions are stable ref-less fetchers

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

  useEffect(() => {
    const unlisten = listen("omnix-navigate-settings", () => {
      setActiveTab("settings");
    });
    return () => { unlisten.then((fn) => fn()); };
  }, []);

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

  const handleTabChange = (tab: string) => {
    setActiveTab(tab);
    if (tab === "cron") {
      cron.loadCronTasks();
      cron.loadCronRuns();
    }
    if (tab === "settings") {
      platforms.loadPlatforms();
      accounts.loadAccounts();
      search.loadProviders();
      mcpServers.loadMcpServers();
      backup.loadBackupInfo();
    }
    if (tab === "team" && convs.currentConvId) {
      const logs = convs.terminalLogsRef.current[convs.currentConvId] || "";
      convs.setCollabLogs(logs);
    }
  };

  // ── Save handlers with user feedback ──────────────

  const handleSaveSettings = async () => {
    try {
      await settings.saveSettings();
      toast.success("设置保存成功！中转代理网关已热重载，外部 Agent 配置文件已同步。");
    } catch (e) {
      toast.error("保存设置失败：" + e);
    }
  };

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

  const handleSwitchAccount = async (id: string) => {
    try {
      await accounts.switchAccount(id);
      toast.success("账号切换成功！中转代理网关已即时切换上游通道。");
    } catch (e) {
      toast.error("切换失败：" + e);
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

  const handleSaveWorkspaceChat = async () => {
    try {
      await convs.saveWorkspaceChat();
    } catch (e) {
      toast.error("新建项目会话失败：" + e);
    }
  };

  // ── Derived state ─────────────────────────────────

  const showConversations = activeTab === "chat" || activeTab === "team";
  const showPreviewButton = !!(convs.chatWorkspace && convs.chatWorkspace !== "direct");

  // ── Render ────────────────────────────────────────

  return (
    <div className="flex w-screen h-screen overflow-hidden">
      <AppSidebar
        activeTab={activeTab}
        onTabChange={handleTabChange}
        gatewayStatus={settings.gatewayStatus}
        showConversations={showConversations}
        conversations={convs.conversations}
        currentConvId={convs.currentConvId}
        activeSessions={convs.activeSessions}
        onSelectConversation={convs.selectConversation}
        onDeleteConversation={convs.deleteConversation}
        onArchiveConversation={convs.archiveConversation}
        onOpenHistoryFullscreen={() => setShowHistoryFullscreen(true)}
        onNewConversation={convs.newConversation}
        onOpenWorkspaceModal={() => convs.setIsWorkspaceModalOpen(true)}
      />

      <main className="flex flex-col flex-1 bg-background relative">
        <AppHeader
          activeTab={activeTab}
          activeAgent={convs.activeAgent}
          chatWorkspace={convs.chatWorkspace}
          showPreviewButton={showPreviewButton}
          isPreviewOpen={preview.showPreviewPane}
          onTogglePreview={() => {
            preview.setShowPreviewPane(!preview.showPreviewPane);
            if (!preview.showPreviewPane) preview.loadPreviewFiles();
          }}
        />

        <div className="flex flex-1 min-w-0 overflow-hidden">
          <Suspense fallback={<LazyFallback />}>
            {activeTab === "dashboard" && (
              <DashboardTab
                activeSessionsCount={convs.activeSessions.length}
                detectedAgents={convs.detectedAgents}
                tipIndex={tipIndex}
                envDiagnostics={diagnostics.envDiagnostics}
                repairLogs={diagnostics.repairLogs}
                repairingTool={diagnostics.repairingTool}
                remoteInfo={remote.remoteInfo}
                onRunDiagnostics={diagnostics.runDiagnostics}
                onRepairTool={diagnostics.repairTool}
                onLoadRemoteAccess={remote.loadRemoteAccess}
              />
            )}

            {activeTab === "chat" && (
              <ChatTab
                activeAgent={convs.activeAgent}
                detectedAgents={convs.detectedAgents}
                messages={convs.messages}
                chatInput={convs.chatInput}
                chatWorkspace={convs.chatWorkspace}
                currentConvId={convs.currentConvId}
                activeSessions={convs.activeSessions}
                promptType={convs.promptType}
                targetModel={settings.targetModel}
                activeModels={platforms.activeModels}
                setActiveAgent={convs.setActiveAgent}
                setChatInput={convs.setChatInput}
                setChatWorkspace={convs.setChatWorkspace}
                setTargetModel={settings.setTargetModel}
                onSendMessage={convs.sendMessage}
                onSendStdinDirect={convs.sendStdinDirect}
                onStopSession={convs.stopAgentSession}
              />
            )}

            {activeTab === "agents" && (
              <AgentHubTab
                detectedAgents={convs.detectedAgents}
                activeAgent={convs.activeAgent}
                accounts={accounts.accounts}
                activeModels={platforms.activeModels}
                onSwitchAgent={convs.setActiveAgent}
                onAddAccount={() => accounts.openAccountModal()}
                onEditAccount={(acc) => accounts.openAccountModal(acc)}
                onDeleteAccount={accounts.deleteAccount}
                onSwitchAccount={handleSwitchAccount}
              />
            )}

            {activeTab === "compare" && <CompareTab />}
            {activeTab === "memories" && <MemoryTab />}
            {activeTab === "skills" && <SkillTab />}
            {activeTab === "knowledge" && <KnowledgeTab />}

            {activeTab === "team" && (
              <TeamTab
                currentConvId={convs.currentConvId}
                conversations={convs.conversations}
                activeAgent={convs.activeAgent}
                detectedAgents={convs.detectedAgents}
                activeSessions={convs.activeSessions}
                collabLogs={convs.collabLogs}
                collabStdin={convs.collabStdin}
                rightPaneWidth={resizer.rightPaneWidth}
                onSelectConversation={convs.selectConversation}
                setActiveAgent={convs.setActiveAgent}
                setCollabStdin={convs.setCollabStdin}
                onStartSession={() => convs.startAgentSession(convs.currentConvId)}
                onStopSession={convs.stopAgentSession}
                onSendStdinDirect={convs.sendStdinDirect}
                startResizing={resizer.startResizing}
              />
            )}

            {activeTab === "cron" && (
              <CronTab
                cronTasks={cron.cronTasks}
                cronRuns={cron.cronRuns}
                onAddTask={() => cron.openCronModal()}
                onEditTask={(task) => cron.openCronModal(task)}
                onDeleteTask={cron.deleteCronTask}
                onToggleTask={cron.toggleCronTask}
                onTriggerTask={cron.triggerCronTask}
                onClearRuns={cron.clearCronRuns}
              />
            )}

            {activeTab === "settings" && (
              <SettingsTab
                settingsSubTab={settingsSubTab}
                setSettingsSubTab={setSettingsSubTab}
                platforms={platforms.platforms}
                selectedPlatformId={platforms.selectedPlatformId}
                platformModels={platforms.platformModels}
                modelTestingState={platforms.modelTestingState}
                fetchingModels={platforms.fetchingModels}
                onSelectPlatform={platforms.selectPlatform}
                onTogglePlatform={platforms.togglePlatform}
                onAddPlatform={() => platforms.openPlatformModal()}
                onEditPlatform={(p) => platforms.openPlatformModal(p)}
                onDeletePlatform={platforms.deletePlatform}
                onFetchRemoteModels={platforms.fetchRemoteModels}
                onAddModel={platforms.openModelModal}
                onToggleModelEnabled={platforms.toggleModelEnabled}
                onTestModel={platforms.testModel}
                onDeleteModel={platforms.deleteModel}
                batchTesting={platforms.batchTesting}
                onBatchTestModels={platforms.batchTestModels}
                accounts={accounts.accounts}
                onAddAccount={() => accounts.openAccountModal()}
                onEditAccount={(acc) => accounts.openAccountModal(acc)}
                onDeleteAccount={accounts.deleteAccount}
                onSwitchAccount={handleSwitchAccount}
                targetModel={settings.targetModel}
                gpuAcceleration={settings.gpuAcceleration}
                idleTimeout={settings.idleTimeout}
                autoStart={settings.autoStart}
                startToTray={settings.startToTray}
                useWsl={settings.useWsl}
                wslDistro={settings.wslDistro}
                setTargetModel={settings.setTargetModel}
                setGpuAcceleration={settings.setGpuAcceleration}
                setIdleTimeout={settings.setIdleTimeout}
                setAutoStart={settings.setAutoStart}
                setStartToTray={settings.setStartToTray}
                setUseWsl={settings.setUseWsl}
                setWslDistro={settings.setWslDistro}
                onSaveSettings={handleSaveSettings}
                selectionCaptureMode={selection.captureMode}
                selectionShowOnCapture={selection.showOnCapture}
                selectionPreserveClipboard={selection.preserveClipboard}
                isSelectionCapturing={selection.isCapturing}
                lastSelectionCapture={selection.lastCapture}
                selectionCaptureError={selection.captureError}
                selectionHistory={selection.selectionHistory}
                onSetSelectionCaptureMode={(v) => selection.saveSelectionSettings({ captureMode: v as "hybrid" | "uia_only" | "clipboard_only" })}
                onSetSelectionShowOnCapture={(v) => selection.saveSelectionSettings({ showOnCapture: v })}
                onSetSelectionPreserveClipboard={(v) => selection.saveSelectionSettings({ preserveClipboard: v })}
                onTestSelectionCapture={selection.captureTextOnly}
                onSaveSelectionSettings={async (updates) => {
                  await selection.saveSelectionSettings(updates as Parameters<typeof selection.saveSelectionSettings>[0]);
                }}
                onLoadSelectionHistory={selection.loadHistory}
                onDeleteSelectionHistoryItem={selection.deleteHistoryItem}
                onClearSelectionHistory={selection.clearHistory}
                translatePreferredLang={translation.preferredLang}
                translateAlterLang={translation.alterLang}
                translateModel={translation.translateModel}
                translateAutoDetect={translation.autoDetect}
                translateCustomPrompt={translation.customPrompt}
                onSetTranslatePreferredLang={(v) => translation.saveTranslationSettings({ preferredLang: v })}
                onSetTranslateAlterLang={(v) => translation.saveTranslationSettings({ alterLang: v })}
                onSetTranslateModel={(v) => translation.saveTranslationSettings({ translateModel: v })}
                onSetTranslateAutoDetect={(v) => translation.saveTranslationSettings({ autoDetect: v })}
                onSetTranslateCustomPrompt={(v) => translation.saveTranslationSettings({ customPrompt: v })}
                onSaveTranslationSettings={async (updates) => {
                  await translation.saveTranslationSettings(updates as Parameters<typeof translation.saveTranslationSettings>[0]);
                }}
                themeMode={settings.themeMode}
                onSetThemeMode={settings.setThemeMode}
                searchProviders={search.providers}
                searchSelectedProviderId={search.selectedProviderId}
                searchResults={search.results}
                searchQuery={search.searchQuery}
                isSearching={search.isSearching}
                onSetSearchQuery={search.setSearchQuery}
                onSetSearchSelectedProviderId={search.setSelectedProviderId}
                onSearch={search.search}
                onAddSearchProvider={() => search.openSearchProviderModal()}
                onEditSearchProvider={(provider) => search.openSearchProviderModal(provider)}
                onDeleteSearchProvider={search.deleteProvider}
                showSearchProviderModal={search.showSearchProviderModal}
                editingSearchProvider={search.editingSearchProvider}
                searchProviderForm={search.searchProviderForm}
                onCloseSearchProviderModal={search.closeSearchProviderModal}
                onUpdateSearchProviderForm={search.updateSearchProviderForm}
                onSaveSearchProvider={async () => {
                  await search.saveProvider({
                    id: search.searchProviderForm.id,
                    name: search.searchProviderForm.name,
                    api_type: search.searchProviderForm.api_type,
                    api_key: search.searchProviderForm.api_key,
                    api_address: search.searchProviderForm.api_address,
                    is_enabled: search.searchProviderForm.is_enabled,
                  });
                  search.closeSearchProviderModal();
                }}
                mcpServers={mcpServers.mcpServers}
                showMcpModal={mcpServers.showMcpModal}
                editingMcpServer={mcpServers.editingMcpServer}
                mcpForm={mcpServers.mcpForm}
                onOpenMcpModal={mcpServers.openMcpModal}
                onCloseMcpModal={mcpServers.closeMcpModal}
                onUpdateMcpForm={mcpServers.updateMcpForm}
                onSaveMcpServer={mcpServers.saveMcpServer}
                onDeleteMcpServer={mcpServers.deleteMcpServer}
                backupTableInfo={backup.tableInfo}
                backupSelectedTables={backup.selectedTables}
                isBackupExporting={backup.isExporting}
                isBackupImporting={backup.isImporting}
                lastImportResult={backup.lastImportResult}
                onLoadBackupInfo={backup.loadBackupInfo}
                onToggleBackupTable={backup.toggleTableSelection}
                onSelectAllBackupTables={backup.selectAllTables}
                onDeselectAllBackupTables={backup.deselectAllTables}
                onExportBackup={backup.exportBackup}
                onImportBackup={backup.importBackup}
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

      {/* Command Palette */}
      <CommandPalette
        open={showCommandPalette}
        onClose={() => setShowCommandPalette(false)}
        onNavigate={(tab) => setActiveTab(tab)}
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
          onArchiveConversation={convs.archiveConversation}
          onUnarchiveConversation={convs.unarchiveConversation}
          onNewConversation={convs.newConversation}
          onLoadArchived={convs.loadArchivedConversations}
          onClose={() => setShowHistoryFullscreen(false)}
        />
      )}
    </div>
  );
}

export default App;

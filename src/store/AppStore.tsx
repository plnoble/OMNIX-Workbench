/**
 * AppStore — 应用级状态的单一入口。
 *
 * 以前所有 hook 都在 `MainApp` 里实例化，然后一层一层往下传 prop：光是
 * `App.tsx` 里就有 50 多行纯透传（`ChatTab` 27 个、`SearchResourceTab` 21 个、
 * 设置那一片 85 个），改一个字段名要同时动 hook 接口、App、组件 props 三处。
 *
 * 这里**不引入任何状态库**——hook 还是原来那些 hook，实例化顺序也一字不改
 * （它们互相有依赖：`useAccounts(platforms.activeModels)`、
 * `useCron(convs.detectedAgents)`…），只是把结果放进一个 Context，让组件
 * 直接取而不是靠上游传。纯接线。
 *
 * 为什么是「一个 Context 装所有域」而不是每个域一个：这些 hook 全在同一个组件
 * 里运行，任何一个变了都会让该组件重渲染——今天靠 prop 传时同样会重渲染所有
 * 已挂载的 Tab（而且同一时刻只有一个 Tab 挂载）。拆成 13 个 Context 不会少一次
 * 渲染，只会多 13 层 Provider 嵌套。
 */

import { createContext, useContext, type ReactNode } from "react";

import {
  useAccounts,
  useBackup,
  useConversations,
  useCron,
  useDiagnostics,
  useMcpServers,
  usePlatforms,
  usePreview,
  useRemoteAccess,
  useSearch,
  useSelection,
  useSettings,
  useTranslation,
  type UseAccountsReturn,
  type UseBackupReturn,
  type UseConversationsReturn,
  type UseCronReturn,
  type UseDiagnosticsReturn,
  type UseMcpServersReturn,
  type UsePlatformsReturn,
  type UsePreviewReturn,
  type UseRemoteAccessReturn,
  type UseSearchReturn,
  type UseSelectionReturn,
  type UseSettingsReturn,
  type UseTranslationReturn,
} from "@/hooks";
import { useAutopilotRunner } from "@/hooks/useAutopilotRunner";

export interface AppStore {
  settings: UseSettingsReturn;
  platforms: UsePlatformsReturn;
  accounts: UseAccountsReturn;
  convs: UseConversationsReturn;
  cron: UseCronReturn;
  preview: UsePreviewReturn;
  diagnostics: UseDiagnosticsReturn;
  remote: UseRemoteAccessReturn;
  selection: UseSelectionReturn;
  translation: UseTranslationReturn;
  search: UseSearchReturn;
  mcpServers: UseMcpServersReturn;
  backup: UseBackupReturn;
}

const AppStoreContext = createContext<AppStore | null>(null);

function useAppStore(): AppStore {
  const store = useContext(AppStoreContext);
  if (!store) {
    throw new Error("store 只能在 <AppStoreProvider> 内部使用");
  }
  return store;
}

// 按域取用。组件写 `const convs = useConversationsStore();`，和以前拿到的
// `convs` prop 是同一个对象，所以组件内部的用法一行都不用改。
export const useSettingsStore = (): UseSettingsReturn => useAppStore().settings;
export const usePlatformsStore = (): UsePlatformsReturn => useAppStore().platforms;
export const useAccountsStore = (): UseAccountsReturn => useAppStore().accounts;
export const useConversationsStore = (): UseConversationsReturn => useAppStore().convs;
export const useCronStore = (): UseCronReturn => useAppStore().cron;
export const usePreviewStore = (): UsePreviewReturn => useAppStore().preview;
export const useDiagnosticsStore = (): UseDiagnosticsReturn => useAppStore().diagnostics;
export const useRemoteAccessStore = (): UseRemoteAccessReturn => useAppStore().remote;
export const useSelectionStore = (): UseSelectionReturn => useAppStore().selection;
export const useTranslationStore = (): UseTranslationReturn => useAppStore().translation;
export const useSearchStore = (): UseSearchReturn => useAppStore().search;
export const useMcpServersStore = (): UseMcpServersReturn => useAppStore().mcpServers;
export const useBackupStore = (): UseBackupReturn => useAppStore().backup;

/**
 * 实例化全部 hook 并向下提供。**顺序与改造前逐行一致**——这些 hook 互相依赖，
 * 换顺序会改变行为。
 */
export function AppStoreProvider({ children }: { children: ReactNode }) {
  const settings = useSettings();
  const platforms = usePlatforms();
  const accounts = useAccounts(platforms.activeModels);
  const convs = useConversations(settings.gatewayStatus);
  // Execute due autopilot runs through the real runtime.
  useAutopilotRunner(convs.loadConversations);
  const cron = useCron(convs.detectedAgents);
  const preview = usePreview(convs.chatWorkspace);
  const diagnostics = useDiagnostics();
  const remote = useRemoteAccess();
  const selection = useSelection();
  const translation = useTranslation();
  const search = useSearch();
  const mcpServers = useMcpServers();
  const backup = useBackup();

  // 刻意不做 useMemo：这些 hook 里绝大多数返回值每次渲染都是新引用，包一层
  // memo 只会造出「看着像稳定、其实每次都变」的假象。上游重渲染时下游本来
  // 也要重渲染（和改造前靠 prop 传是同一回事）。
  const store: AppStore = {
    settings,
    platforms,
    accounts,
    convs,
    cron,
    preview,
    diagnostics,
    remote,
    selection,
    translation,
    search,
    mcpServers,
    backup,
  };

  return <AppStoreContext.Provider value={store}>{children}</AppStoreContext.Provider>;
}

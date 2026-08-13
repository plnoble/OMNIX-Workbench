import { Database, KeyRound, RefreshCw, ShieldCheck } from "lucide-react";

import { GatewayHealthCard } from "@/components/GatewayHealthCard";
import { PlatformSubTab, type PlatformSubTabProps } from "@/components/tabs/SettingsTab";
import { usePlatformsStore } from "@/store/AppStore";

export function ModelsTab() {
  // `PlatformSubTab` 是共享叶子（模型中心 + 设置页各渲染一次），它的 props 形状
  // 由设置页那个大接口定义，所以这里不动它——只把 App.tsx 那 17 行透传收进来。
  const platforms = usePlatformsStore();
  const props: PlatformSubTabProps = {
    platforms: platforms.platforms,
    selectedPlatformId: platforms.selectedPlatformId,
    platformModels: platforms.platformModels,
    modelTestingState: platforms.modelTestingState,
    fetchingModels: platforms.fetchingModels,
    onSelectPlatform: platforms.selectPlatform,
    onTogglePlatform: platforms.togglePlatform,
    onAddPlatform: () => platforms.openPlatformModal(),
    onEditPlatform: (p) => platforms.openPlatformModal(p),
    onDeletePlatform: platforms.deletePlatform,
    onFetchRemoteModels: platforms.fetchRemoteModels,
    onAddModel: platforms.openModelModal,
    onToggleModelEnabled: platforms.toggleModelEnabled,
    onTestModel: platforms.testModel,
    onDeleteModel: platforms.deleteModel,
    batchTesting: platforms.batchTesting,
    onBatchTestModels: platforms.batchTestModels,
  };
  return (
    <div className="flex h-full flex-1 flex-col overflow-hidden bg-background">
      <div className="border-b border-border px-6 py-4">
        <div className="flex flex-wrap items-center justify-between gap-4">
          <div className="min-w-0">
            <div className="flex items-center gap-2 text-lg font-semibold">
              <Database className="h-5 w-5 text-primary" />
              模型中心
            </div>
            <p className="mt-1 max-w-3xl text-sm leading-6 text-muted-foreground">
              管理供应商、API 地址、多 Key、模型列表、能力识别和健康检查。系统设置不再放在这里。
            </p>
          </div>
          <div className="grid grid-cols-3 gap-2 text-xs text-muted-foreground">
            <div className="rounded-md border border-border glass-surface px-3 py-2">
              <KeyRound className="mb-1 h-3.5 w-3.5" />
              主 Key + 故障切换
            </div>
            <div className="rounded-md border border-border glass-surface px-3 py-2">
              <RefreshCw className="mb-1 h-3.5 w-3.5" />
              从地址拉取模型
            </div>
            <div className="rounded-md border border-border glass-surface px-3 py-2">
              <ShieldCheck className="mb-1 h-3.5 w-3.5" />
              批量健康检查
            </div>
          </div>
        </div>
      </div>

      <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-hidden p-5">
        <GatewayHealthCard />
        <div className="min-h-0 flex-1 overflow-hidden">
          <PlatformSubTab {...props} />
        </div>
      </div>
    </div>
  );
}

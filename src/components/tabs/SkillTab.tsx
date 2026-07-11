/**
 * SkillTab — 技能中心.
 *
 * 一页流水线：收集 → 待定池（摘要+审核）→ 改造/融合 → 晋升 → 正式池网关直调。
 * 市场（外部导入）与工具同步（物理分发）作为辅助面板按需打开。
 */
import { useState } from "react";
import { ArrowRightLeft, Sparkles, Store } from "lucide-react";

import { SkillPoolPanel } from "@/components/SkillPoolPanel";
import { SkillMarketPanel } from "@/components/SkillMarketPanel";
import { SkillSyncPanel } from "@/components/SkillSyncPanel";

export function SkillTab() {
  const [showMarket, setShowMarket] = useState(false);
  const [showSync, setShowSync] = useState(false);
  // 让市场导入后技能中心刷新：换 key 重挂载最省事且无状态耦合。
  const [poolKey, setPoolKey] = useState(0);

  return (
    <div className="flex h-full flex-1 min-w-0 flex-col overflow-hidden bg-background">
      <div className="flex items-center gap-2 border-b border-border px-6 py-4">
        <Sparkles className="h-5 w-5 text-primary" />
        <div>
          <div className="text-lg font-semibold">技能中心</div>
          <p className="text-xs text-muted-foreground">
            收集 → 待定池（看得懂）→ 审核（给出问题与改法）→ AI 改造/融合 → 你拍板晋升 → 正式池全 agent 直调
          </p>
        </div>
        <div className="ml-auto flex items-center gap-1.5">
          <button
            onClick={() => setShowMarket(true)}
            className="inline-flex h-8 items-center gap-1.5 rounded-md border border-border px-2.5 text-xs text-muted-foreground hover:bg-muted/40 hover:text-foreground"
            title="从 GitHub 等来源搜索并导入技能（进待定池）"
          >
            <Store className="h-3.5 w-3.5" /> 市场
          </button>
          <button
            onClick={() => setShowSync(true)}
            className="inline-flex h-8 items-center gap-1.5 rounded-md border border-border px-2.5 text-xs text-muted-foreground hover:bg-muted/40 hover:text-foreground"
            title="把技能物理分发到不走网关的工具目录"
          >
            <ArrowRightLeft className="h-3.5 w-3.5" /> 同步
          </button>
        </div>
      </div>
      <div className="min-h-0 flex-1 overflow-hidden p-4">
        <SkillPoolPanel key={poolKey} />
      </div>

      {showMarket && (
        <SkillMarketPanel
          onClose={() => setShowMarket(false)}
          onImported={() => setPoolKey((k) => k + 1)}
        />
      )}
      {showSync && <SkillSyncPanel onClose={() => setShowSync(false)} />}
    </div>
  );
}

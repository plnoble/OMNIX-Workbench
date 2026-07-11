/**
 * SkillTab — 技能中心 (R2 重构).
 *
 * 新体系一页流：收集 → 待定池（摘要+审核）→ 改造/融合 → 晋升 → 正式池网关直调。
 * 旧的多分区页（市场/同步/熔炉/组合…）保留一个入口，默认不展示。
 */
import { lazy, Suspense, useState } from "react";
import { Sparkles } from "lucide-react";

import { SkillPoolPanel } from "@/components/SkillPoolPanel";

const LegacySkillHub = lazy(() =>
  import("@/SkillHub").then((m) => ({ default: m.SkillHub })),
);

export function SkillTab() {
  const [legacy, setLegacy] = useState(false);

  if (legacy) {
    return (
      <div className="flex h-full flex-1 min-w-0 flex-col overflow-hidden">
        <div className="flex items-center gap-2 border-b border-border px-4 py-1.5 text-xs text-muted-foreground">
          旧版技能页（市场/同步/熔炉等）
          <button
            onClick={() => setLegacy(false)}
            className="ml-auto rounded border border-border px-2 py-0.5 hover:bg-muted/40"
          >
            返回技能中心
          </button>
        </div>
        <Suspense fallback={<div className="p-6 text-sm text-muted-foreground">加载中…</div>}>
          <div className="flex min-h-0 flex-1 overflow-hidden">
            <LegacySkillHub />
          </div>
        </Suspense>
      </div>
    );
  }

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
        <button
          onClick={() => setLegacy(true)}
          className="ml-auto rounded-md border border-border px-2 py-1 text-xs text-muted-foreground hover:bg-muted/40"
          title="市场搜索/工具同步/熔炉等旧功能"
        >
          旧版功能…
        </button>
      </div>
      <div className="min-h-0 flex-1 overflow-hidden p-4">
        <SkillPoolPanel />
      </div>
    </div>
  );
}

import { useEffect, useState } from "react";
import {
  AlertTriangle,
  ArrowRight,
  Clock,
  Code2,
  FlaskConical,
  GitCompare,
  RefreshCw,
  Rocket,
  Sparkles,
} from "lucide-react";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { toast } from "@/components/ui/sonner";
import { labsApi } from "@/lib/tauri-api";
import type { LabFeature } from "@/types";

interface LabsTabProps {
  onNavigate: (tab: string) => void;
}

const LAB_NAV: Record<string, { tab?: string; icon: typeof FlaskConical }> = {
  compare: { tab: "compare", icon: GitCompare },
  cron: { tab: "cron", icon: Clock },
  autopilot: { icon: Rocket },
  "skill-evolution": { tab: "skills", icon: Sparkles },
  cookbook: { icon: FlaskConical },
  "code-analysis": { icon: Code2 },
};

export function LabsTab({ onNavigate }: LabsTabProps) {
  const [features, setFeatures] = useState<LabFeature[]>([]);
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    loadFeatures();
  }, []);

  async function loadFeatures() {
    setLoading(true);
    try {
      const nextFeatures = await labsApi.listFeatures();
      setFeatures(nextFeatures.filter((feature) => feature.is_visible));
    } catch (error) {
      toast.error(`加载 Labs 失败：${String(error)}`);
    } finally {
      setLoading(false);
    }
  }

  return (
    <div className="flex-1 overflow-y-auto bg-background">
      <div className="mx-auto flex w-full max-w-[1320px] flex-col gap-4 p-5">
        <section className="rounded-md border border-border bg-card/55 p-4">
          <div className="flex flex-wrap items-start justify-between gap-3">
            <div>
              <div className="flex items-center gap-2 text-base font-semibold">
                <FlaskConical className="h-5 w-5 text-warning" />
                Labs 实验区
              </div>
              <p className="mt-1 max-w-3xl text-sm leading-6 text-muted-foreground">
                这些能力保留可见，但默认不进入核心 Workbench 主流程。每个功能都会标记完成度、风险和当前接入状态。
              </p>
            </div>
            <Button size="sm" variant="outline" onClick={loadFeatures}>
              <RefreshCw className={loading ? "h-4 w-4 animate-spin" : "h-4 w-4"} />
              刷新
            </Button>
          </div>
        </section>

        <section className="grid gap-3 md:grid-cols-2 xl:grid-cols-3">
          {features.map((feature) => {
            const nav = LAB_NAV[feature.id] ?? { icon: FlaskConical };
            const Icon = nav.icon;
            const canOpen = Boolean(nav.tab);

            return (
              <article key={feature.id} className="rounded-md border border-border bg-card/55 p-4">
                <div className="flex items-start justify-between gap-3">
                  <div className="flex min-w-0 items-center gap-2">
                    <Icon className="h-5 w-5 shrink-0 text-accent" />
                    <h3 className="truncate text-sm font-semibold">{feature.title}</h3>
                  </div>
                  <RiskBadge risk={feature.risk} />
                </div>
                <div className="mt-3 flex flex-wrap gap-2">
                  <StatusBadge status={feature.status} />
                  <Badge variant="outline">{feature.layer}</Badge>
                </div>
                <p className="mt-3 min-h-[72px] text-sm leading-6 text-muted-foreground">
                  {feature.description}
                </p>
                <div className="mt-4 flex items-center justify-between gap-3 border-t border-border pt-3">
                  <div className="flex items-center gap-1.5 text-xs text-muted-foreground">
                    <AlertTriangle className="h-3.5 w-3.5" />
                    Experimental / Incomplete
                  </div>
                  <Button
                    size="sm"
                    variant={canOpen ? "outline" : "secondary"}
                    disabled={!canOpen}
                    onClick={() => nav.tab && onNavigate(nav.tab)}
                  >
                    {canOpen ? "打开" : "待接入"}
                    {canOpen && <ArrowRight className="h-3.5 w-3.5" />}
                  </Button>
                </div>
              </article>
            );
          })}
        </section>
      </div>
    </div>
  );
}

function StatusBadge({ status }: { status: string }) {
  const normalized = status.toLowerCase();
  const labelMap: Record<string, string> = {
    experimental: "Experimental",
    incomplete: "Incomplete",
    preview: "Preview",
  };
  const variant: "warning" | "secondary" =
    normalized === "incomplete" || normalized === "experimental" ? "warning" : "secondary";
  return <Badge variant={variant}>{labelMap[normalized] ?? status}</Badge>;
}

function RiskBadge({ risk }: { risk: string }) {
  const normalized = risk.toLowerCase();
  const variant: "destructive" | "warning" | "success" | "secondary" =
    normalized === "high"
      ? "destructive"
      : normalized === "medium"
        ? "warning"
        : normalized === "low"
          ? "success"
          : "secondary";
  const labelMap: Record<string, string> = {
    high: "高风险",
    medium: "中风险",
    low: "低风险",
  };
  return <Badge variant={variant}>{labelMap[normalized] ?? risk}</Badge>;
}

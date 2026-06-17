/**
 * SkillTab — 自进化技能 (thin wrapper)
 */

import { SkillHub } from "@/SkillHub";

export function SkillTab() {
  return (
    <div className="flex h-full flex-1 min-w-0 overflow-hidden">
      <SkillHub />
    </div>
  );
}

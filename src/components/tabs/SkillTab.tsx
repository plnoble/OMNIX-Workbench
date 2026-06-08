/**
 * SkillTab — 自进化技能 (thin wrapper)
 */

import { SkillHub } from "@/SkillHub";

export function SkillTab() {
  return (
    <div className="flex-1 overflow-hidden">
      <SkillHub />
    </div>
  );
}

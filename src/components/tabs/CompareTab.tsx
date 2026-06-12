/**
 * CompareTab — AI 专家比对中枢 (thin wrapper)
 */

import { CompareHub } from "@/CompareHub";

export function CompareTab() {
  return (
    <div className="flex-1 min-w-0 overflow-hidden">
      <CompareHub />
    </div>
  );
}

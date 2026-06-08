/**
 * CompareTab — AI 专家比对中枢 (thin wrapper)
 */

import { CompareHub } from "@/CompareHub";

interface CompareTabProps {
  proxyPort: string;
}

export function CompareTab({ proxyPort }: CompareTabProps) {
  return (
    <div className="flex-1 overflow-hidden">
      <CompareHub proxyPort={proxyPort} />
    </div>
  );
}

/**
 * MemoryTab — 长期避坑记忆库 (thin wrapper)
 */

import { MemoryHub } from "@/MemoryHub";

export function MemoryTab() {
  return (
    <div className="flex-1 overflow-hidden">
      <MemoryHub />
    </div>
  );
}

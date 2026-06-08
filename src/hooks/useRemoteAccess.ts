/**
 * useRemoteAccess — Remote cross-device debugging info
 */

import { useState, useCallback } from "react";
import { remoteApi } from "@/lib/tauri-api";
import type { RemoteAccessInfo } from "@/types";

export interface UseRemoteAccessReturn {
  remoteInfo: RemoteAccessInfo | null;
  loadRemoteAccess: () => Promise<void>;
}

export function useRemoteAccess(): UseRemoteAccessReturn {
  const [remoteInfo, setRemoteInfo] = useState<RemoteAccessInfo | null>(null);

  const loadRemoteAccess = useCallback(async () => {
    try {
      const info = await remoteApi.getInfo();
      setRemoteInfo(info);
    } catch (e) {
      console.error("[useRemoteAccess] Failed to get remote access info:", e);
    }
  }, []);

  return { remoteInfo, loadRemoteAccess };
}

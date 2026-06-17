import { useCallback, useEffect, useRef, useState } from "react";

import { DEFAULT_NAVIGATION_LAYOUT, normalizeNavigationLayout } from "@/lib/appRegistry";
import { settingsApi } from "@/lib/tauri-api";
import type { NavigationLayout, NavigationPlacement } from "@/types";

const NAVIGATION_LAYOUT_KEY = "ui.navigation.layout";

export function useNavigationLayout() {
  const [layout, setLayout] = useState<NavigationLayout>(DEFAULT_NAVIGATION_LAYOUT);
  const layoutRef = useRef<NavigationLayout>(DEFAULT_NAVIGATION_LAYOUT);

  useEffect(() => {
    layoutRef.current = layout;
  }, [layout]);

  const persist = useCallback(async (next: NavigationLayout) => {
    const normalized = normalizeNavigationLayout(next);
    layoutRef.current = normalized;
    setLayout(normalized);
    await settingsApi.set(NAVIGATION_LAYOUT_KEY, JSON.stringify(normalized));
  }, []);

  const loadLayout = useCallback(async () => {
    try {
      const raw = await settingsApi.get(NAVIGATION_LAYOUT_KEY);
      if (!raw) {
        layoutRef.current = DEFAULT_NAVIGATION_LAYOUT;
        setLayout(DEFAULT_NAVIGATION_LAYOUT);
        return;
      }
      const normalized = normalizeNavigationLayout(JSON.parse(raw));
      layoutRef.current = normalized;
      setLayout(normalized);
    } catch (error) {
      console.error("[useNavigationLayout] Failed to load layout:", error);
      layoutRef.current = DEFAULT_NAVIGATION_LAYOUT;
      setLayout(DEFAULT_NAVIGATION_LAYOUT);
    }
  }, []);

  const moveEntry = useCallback(async (id: string, placement: NavigationPlacement) => {
    const current = layoutRef.current;
    const without = {
      pinned: current.pinned.filter((item) => item !== id),
      launcher: current.launcher.filter((item) => item !== id),
      hidden: current.hidden.filter((item) => item !== id),
    };

    if (id === "work" && placement === "hidden") {
      placement = "pinned";
    }

    const next: NavigationLayout = {
      ...without,
      [placement]: [...without[placement], id],
    };
    await persist(next);
  }, [persist]);

  const reorderEntry = useCallback(async (id: string, direction: "left" | "right") => {
    const current = layoutRef.current;
    const placements: NavigationPlacement[] = ["pinned", "launcher", "hidden"];
    const placement = placements.find((item) => current[item].includes(id));

    if (!placement) return;

    const items = [...current[placement]];
    const index = items.indexOf(id);
    const nextIndex = direction === "left" ? index - 1 : index + 1;

    if (index < 0 || nextIndex < 0 || nextIndex >= items.length) return;

    items.splice(index, 1);
    items.splice(nextIndex, 0, id);

    await persist({
      ...current,
      [placement]: items,
    });
  }, [persist]);

  const resetLayout = useCallback(async () => {
    await persist(DEFAULT_NAVIGATION_LAYOUT);
  }, [persist]);

  return {
    layout,
    loadLayout,
    moveEntry,
    reorderEntry,
    resetLayout,
  };
}

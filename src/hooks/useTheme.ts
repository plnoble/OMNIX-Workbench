/**
 * useTheme — Apply theme mode to document and react to OS changes
 *
 * Supports "dark" (default), "light", and "auto" (follows OS preference).
 * Sets `data-theme` attribute on <html> to drive CSS variable overrides.
 */

import { useEffect } from "react";

export type ThemeMode = "dark" | "light" | "auto";

export function useTheme(mode: ThemeMode) {
  useEffect(() => {
    const effective =
      mode === "auto"
        ? window.matchMedia("(prefers-color-scheme: dark)").matches
          ? "dark"
          : "light"
        : mode;
    document.documentElement.setAttribute("data-theme", effective);
  }, [mode]);

  // Listen for OS theme changes in "auto" mode
  useEffect(() => {
    if (mode !== "auto") return;

    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    const handler = (e: MediaQueryListEvent) => {
      document.documentElement.setAttribute(
        "data-theme",
        e.matches ? "dark" : "light"
      );
    };
    mq.addEventListener("change", handler);
    return () => mq.removeEventListener("change", handler);
  }, [mode]);
}

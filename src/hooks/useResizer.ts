/**
 * useResizer — Draggable splitter resize logic for Team tab
 */

import { useState, useCallback, useRef } from "react";

export interface UseResizerReturn {
  rightPaneWidth: number;
  startResizing: (e: React.MouseEvent) => void;
}

export function useResizer(): UseResizerReturn {
  const [rightPaneWidth, setRightPaneWidth] = useState(500);
  const isResizingRef = useRef(false);

  const handleMouseMove = useCallback((e: MouseEvent) => {
    if (!isResizingRef.current) return;
    const windowWidth = window.innerWidth;
    const newWidth = windowWidth - e.clientX - 100;
    if (newWidth > 200 && newWidth < windowWidth - 300) {
      setRightPaneWidth(newWidth);
    }
  }, []);

  const stopResizing = useCallback(() => {
    isResizingRef.current = false;
    document.removeEventListener("mousemove", handleMouseMove);
    document.removeEventListener("mouseup", stopResizing);
  }, [handleMouseMove]);

  const startResizing = useCallback((e: React.MouseEvent) => {
    isResizingRef.current = true;
    document.addEventListener("mousemove", handleMouseMove);
    document.addEventListener("mouseup", stopResizing);
    e.preventDefault();
  }, [handleMouseMove, stopResizing]);

  return { rightPaneWidth, startResizing };
}

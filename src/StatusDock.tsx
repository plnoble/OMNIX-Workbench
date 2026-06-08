import React, { useState, useEffect } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { invoke } from "@tauri-apps/api/core";
import { listen, emit } from "@tauri-apps/api/event";
import { LogicalSize } from "@tauri-apps/api/dpi";
import { cn } from "@/lib/utils";

type DevStatus = "idle" | "busy" | "pending" | "error";

interface StatusChangeEvent {
  status: DevStatus;
  text: string;
  approvalMode: "auto" | "manual" | "plan";
  keepAwake: boolean;
}

const DOCK_W = 200;
const DOCK_H = 48;
const MENU_H = 220;

// Opacity presets
const OPACITY_LEVELS = [1.0, 0.75, 0.5] as const;
const OPACITY_LABELS = ["100%", "75%", "50%"] as const;

export default function StatusDock() {
  const [status, setStatus] = useState<DevStatus>("idle");
  const [activeAgentText, setActiveAgentText] = useState<string>("就绪");
  const [approvalMode, setApprovalMode] = useState<"auto" | "manual" | "plan">("auto");

  // Context menu state
  const [menuVisible, setMenuVisible] = useState(false);
  // Dock opacity
  const [opacityIdx, setOpacityIdx] = useState(0);

  const appWindow = getCurrentWindow();

  // 1. Listen to global status updates from App.tsx
  useEffect(() => {
    const unlisten = listen<StatusChangeEvent>("omnix-dev-status-change", (event) => {
      setStatus(event.payload.status);
      setActiveAgentText(event.payload.text);
      setApprovalMode(event.payload.approvalMode);
    });
    return () => { unlisten.then((fn) => fn()); };
  }, []);

  // 2. Close context menu on outside click
  useEffect(() => {
    if (!menuVisible) return;
    const handleGlobalClick = () => {
      setMenuVisible(false);
      appWindow.setSize(new LogicalSize(DOCK_W, DOCK_H)).catch(console.error);
    };
    const id = setTimeout(() => {
      window.addEventListener("click", handleGlobalClick);
    }, 50);
    return () => {
      clearTimeout(id);
      window.removeEventListener("click", handleGlobalClick);
    };
  }, [menuVisible]);

  // Dragging handler — entire card
  const handleDragStart = async (e: React.MouseEvent) => {
    if (e.button !== 0) return;
    if (menuVisible) return;
    e.preventDefault();
    try {
      await appWindow.startDragging();
    } catch (err) {
      console.error("StatusDock: startDragging failed:", err);
    }
  };

  // Click on card → focus main window
  const handleCardClick = async () => {
    if (menuVisible) return;
    try {
      await invoke("focus_main_window");
    } catch (err) {
      console.error("Failed to focus main window:", err);
    }
  };

  // Context Menu
  const handleContextMenu = async (e: React.MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    setMenuVisible(true);
    try {
      await appWindow.setSize(new LogicalSize(DOCK_W, MENU_H));
    } catch (err) {
      console.error("Failed to resize for menu:", err);
    }
  };

  const closeMenu = async () => {
    setMenuVisible(false);
    try {
      await appWindow.setSize(new LogicalSize(DOCK_W, DOCK_H));
    } catch (err) { console.error(err); }
  };

  // ─── Menu actions ───
  const handleToggleApproval = async () => {
    emit("omnix-action-toggle-approval");
    await closeMenu();
  };

  const handleNewConversation = async () => {
    // Emit event to App.tsx to create a new conversation and switch to team tab
    emit("omnix-action-new-conversation");
    await closeMenu();
  };

  const handleCycleOpacity = async () => {
    const nextIdx = (opacityIdx + 1) % OPACITY_LEVELS.length;
    setOpacityIdx(nextIdx);
    // Don't close menu — let user see the change
  };

  const handleOpenSettings = async () => {
    // Emit event to App.tsx to switch to settings tab and focus main window
    emit("omnix-action-open-settings");
    try { await invoke("focus_main_window"); } catch {}
    await closeMenu();
  };

  const handleHideDock = async () => {
    try {
      await invoke("toggle_status_dock", { visible: false });
    } catch (e) {
      console.error("Failed to hide StatusDock:", e);
    }
    await closeMenu();
  };

  const cfg = getStatusColorConfig(status);

  return (
    <div
      className="relative w-full h-full"
      style={{
        overflow: "visible",
        background: "transparent",
        opacity: OPACITY_LEVELS[opacityIdx],
        transition: "opacity 0.3s ease",
      }}
    >
      {/* Main Dock Card */}
      <div
        onMouseDown={handleDragStart}
        onClick={handleCardClick}
        onContextMenu={handleContextMenu}
        title="左键拖动 | 单击唤起主窗口 | 右键打开菜单"
        className="flex items-center cursor-grab select-none box-border"
        style={{
          width: `${DOCK_W}px`,
          height: `${DOCK_H}px`,
          borderRadius: "24px",
          background: "rgba(10, 12, 22, 0.88)",
          backdropFilter: "blur(24px)",
          WebkitBackdropFilter: "blur(24px)",
          border: `1px solid ${cfg.borderGlow}`,
          boxShadow: `0 4px 20px rgba(0, 0, 0, 0.5), 0 0 15px ${cfg.bgGlow}`,
          padding: "0 14px",
          transition: "border-color 0.4s ease, box-shadow 0.4s ease",
        }}
      >
        {/* Status Dot */}
        <div className="relative shrink-0 mr-2.5" style={{ width: "12px", height: "12px" }}>
          {cfg.shouldPulse && (
            <div
              className="absolute rounded-full"
              style={{
                inset: "-3px",
                border: `1.5px solid ${cfg.color}`,
                opacity: 0.5,
                animation: `sd-pulse-ring ${cfg.pulseSpeed} infinite ease-in-out`,
              }}
            />
          )}
          <div
            className="absolute rounded-full"
            style={{
              inset: 0,
              backgroundColor: cfg.color,
              boxShadow: `0 0 8px ${cfg.color}`,
              animation: cfg.shouldPulse ? `sd-pulse-dot ${cfg.pulseSpeed} infinite ease-in-out` : "none",
            }}
          />
          <div
            className="absolute rounded-full"
            style={{
              width: "5px",
              height: "5px",
              top: "3.5px",
              left: "3.5px",
              background: "rgba(255,255,255,0.65)",
              zIndex: 2,
            }}
          />
        </div>

        {/* Text Area */}
        <div className="flex flex-col grow min-w-0">
          <span className="text-xs font-bold text-white leading-tight" style={{ letterSpacing: "0.3px" }}>
            OMNIX DevFlow
          </span>
          <span
            className="text-[9px] font-medium leading-snug whitespace-nowrap truncate opacity-85"
            style={{
              color: cfg.color,
              transition: "color 0.3s ease",
            }}
          >
            {activeAgentText}
          </span>
        </div>

        {/* Grip dots */}
        <div className="flex flex-col opacity-35 ml-1.5 shrink-0" style={{ gap: "2px" }}>
          {[0, 1, 2].map(row => (
            <div key={row} className="flex" style={{ gap: "2px" }}>
              <div className="rounded-full bg-white" style={{ width: "2px", height: "2px" }} />
              <div className="rounded-full bg-white" style={{ width: "2px", height: "2px" }} />
            </div>
          ))}
        </div>
      </div>

      {/* Context Menu */}
      {menuVisible && (
        <div
          className="absolute rounded-xl box-border"
          style={{
            left: "10px",
            top: `${DOCK_H + 4}px`,
            width: `${DOCK_W - 20}px`,
            background: "rgba(12, 15, 28, 0.92)",
            backdropFilter: "blur(24px)",
            WebkitBackdropFilter: "blur(24px)",
            border: "1px solid rgba(255, 255, 255, 0.08)",
            boxShadow: "0 10px 30px rgba(0,0,0,0.55), inset 0 1px 0 rgba(255,255,255,0.05)",
            padding: "5px 0",
            zIndex: 1000,
            animation: "sd-fadeIn 0.12s ease-out",
          }}
          onClick={(e) => e.stopPropagation()}
          onMouseDown={(e) => e.stopPropagation()}
        >
          <MenuItem onClick={handleToggleApproval}>
            🛡️ 审批: {approvalMode === "auto" ? "全自动" : approvalMode === "manual" ? "手动确认" : "计划模式"}
          </MenuItem>

          <MenuItem onClick={handleNewConversation}>
            📋 快速新建对话
          </MenuItem>

          <MenuItem onClick={handleCycleOpacity}>
            🎨 透明度: {OPACITY_LABELS[opacityIdx]} → {OPACITY_LABELS[(opacityIdx + 1) % OPACITY_LEVELS.length]}
          </MenuItem>

          <MenuItem onClick={handleOpenSettings}>
            📊 打开设置面板
          </MenuItem>

          <div className="h-px my-1" style={{ background: "rgba(255,255,255,0.06)" }} />

          <MenuItem onClick={handleHideDock} muted>
            ❌ 隐藏悬浮栏
          </MenuItem>
        </div>
      )}

      {/* Scoped CSS */}
      <style>{`
        @keyframes sd-pulse-ring {
          0% { transform: scale(0.85); opacity: 0.5; }
          50% { transform: scale(1.35); opacity: 0.15; }
          100% { transform: scale(0.85); opacity: 0.5; }
        }
        @keyframes sd-pulse-dot {
          0% { transform: scale(0.92); opacity: 0.7; }
          50% { transform: scale(1.08); opacity: 1; }
          100% { transform: scale(0.92); opacity: 0.7; }
        }
        @keyframes sd-fadeIn {
          from { opacity: 0; transform: translateY(-4px); }
          to { opacity: 1; transform: translateY(0); }
        }
      `}</style>
    </div>
  );
}

/* ─── Helpers ─── */
function getStatusColorConfig(status: DevStatus) {
  switch (status) {
    case "busy":
      return { color: "#10b981", borderGlow: "rgba(16,185,129,0.3)", bgGlow: "rgba(16,185,129,0.15)", shouldPulse: true, pulseSpeed: "1.5s" };
    case "pending":
      return { color: "#f59e0b", borderGlow: "rgba(245,158,11,0.3)", bgGlow: "rgba(245,158,11,0.15)", shouldPulse: true, pulseSpeed: "1.0s" };
    case "error":
      return { color: "#ef4444", borderGlow: "rgba(239,68,68,0.3)", bgGlow: "rgba(239,68,68,0.15)", shouldPulse: true, pulseSpeed: "0.8s" };
    case "idle":
    default:
      return { color: "#10b981", borderGlow: "rgba(16,185,129,0.12)", bgGlow: "rgba(16,185,129,0.06)", shouldPulse: false, pulseSpeed: "0s" };
  }
}

function MenuItem({ children, onClick, disabled, muted }: {
  children: React.ReactNode;
  onClick?: () => void;
  disabled?: boolean;
  muted?: boolean;
}) {
  const [hovered, setHovered] = useState(false);
  let color = "rgba(255,255,255,0.85)";
  if (disabled) color = "rgba(255,255,255,0.25)";
  else if (muted) color = "rgba(255,255,255,0.5)";

  return (
    <div
      onClick={disabled ? undefined : onClick}
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
      className={cn(
        "py-2 px-3 text-[10px] font-medium select-none",
        disabled ? "cursor-not-allowed" : "cursor-pointer"
      )}
      style={{
        color,
        background: hovered && !disabled ? "rgba(255, 255, 255, 0.06)" : "transparent",
        transition: "background 0.15s, color 0.15s",
      }}
    >
      {children}
    </div>
  );
}

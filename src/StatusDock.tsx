import React, { useState, useEffect } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { invoke } from "@tauri-apps/api/core";
import { listen, emit } from "@tauri-apps/api/event";
import { LogicalSize } from "@tauri-apps/api/dpi";

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
      style={{
        position: "relative",
        width: "100vw",
        height: "100vh",
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
        style={{
          width: `${DOCK_W}px`,
          height: `${DOCK_H}px`,
          borderRadius: "24px",
          background: "rgba(10, 12, 22, 0.88)",
          backdropFilter: "blur(24px)",
          WebkitBackdropFilter: "blur(24px)",
          border: `1px solid ${cfg.borderGlow}`,
          boxShadow: `0 4px 20px rgba(0, 0, 0, 0.5), 0 0 15px ${cfg.bgGlow}`,
          display: "flex",
          alignItems: "center",
          padding: "0 14px",
          cursor: "grab",
          userSelect: "none",
          boxSizing: "border-box",
          transition: "border-color 0.4s ease, box-shadow 0.4s ease",
        }}
      >
        {/* Status Dot */}
        <div style={{ position: "relative", width: "12px", height: "12px", flexShrink: 0, marginRight: "10px" }}>
          {cfg.shouldPulse && (
            <div
              style={{
                position: "absolute",
                inset: "-3px",
                borderRadius: "50%",
                border: `1.5px solid ${cfg.color}`,
                opacity: 0.5,
                animation: `sd-pulse-ring ${cfg.pulseSpeed} infinite ease-in-out`,
              }}
            />
          )}
          <div
            style={{
              position: "absolute",
              inset: 0,
              borderRadius: "50%",
              backgroundColor: cfg.color,
              boxShadow: `0 0 8px ${cfg.color}`,
              animation: cfg.shouldPulse ? `sd-pulse-dot ${cfg.pulseSpeed} infinite ease-in-out` : "none",
            }}
          />
          <div
            style={{
              position: "absolute",
              width: "5px",
              height: "5px",
              top: "3.5px",
              left: "3.5px",
              borderRadius: "50%",
              background: "rgba(255,255,255,0.65)",
              zIndex: 2,
            }}
          />
        </div>

        {/* Text Area */}
        <div style={{ display: "flex", flexDirection: "column", minWidth: 0, flexGrow: 1 }}>
          <span style={{ fontSize: "11px", fontWeight: 700, color: "#fff", lineHeight: "1.2", letterSpacing: "0.3px" }}>
            OMNIX DevFlow
          </span>
          <span style={{
            fontSize: "9px",
            color: cfg.color,
            fontWeight: 500,
            lineHeight: "1.3",
            whiteSpace: "nowrap",
            textOverflow: "ellipsis",
            overflow: "hidden",
            opacity: 0.85,
            transition: "color 0.3s ease",
          }}>
            {activeAgentText}
          </span>
        </div>

        {/* Grip dots */}
        <div style={{ display: "flex", flexDirection: "column", gap: "2px", opacity: 0.35, marginLeft: "6px", flexShrink: 0 }}>
          {[0, 1, 2].map(row => (
            <div key={row} style={{ display: "flex", gap: "2px" }}>
              <div style={{ width: "2px", height: "2px", borderRadius: "50%", background: "#fff" }} />
              <div style={{ width: "2px", height: "2px", borderRadius: "50%", background: "#fff" }} />
            </div>
          ))}
        </div>
      </div>

      {/* Context Menu */}
      {menuVisible && (
        <div
          style={{
            position: "absolute",
            left: "10px",
            top: `${DOCK_H + 4}px`,
            width: `${DOCK_W - 20}px`,
            background: "rgba(12, 15, 28, 0.92)",
            backdropFilter: "blur(24px)",
            WebkitBackdropFilter: "blur(24px)",
            border: "1px solid rgba(255, 255, 255, 0.08)",
            borderRadius: "12px",
            boxShadow: "0 10px 30px rgba(0,0,0,0.55), inset 0 1px 0 rgba(255,255,255,0.05)",
            padding: "5px 0",
            zIndex: 1000,
            animation: "sd-fadeIn 0.12s ease-out",
            boxSizing: "border-box",
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

          <div style={{ height: "1px", background: "rgba(255,255,255,0.06)", margin: "4px 0" }} />

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
      style={{
        padding: "8px 12px",
        fontSize: "10px",
        fontWeight: 500,
        color,
        cursor: disabled ? "not-allowed" : "pointer",
        background: hovered && !disabled ? "rgba(255, 255, 255, 0.06)" : "transparent",
        transition: "background 0.15s, color 0.15s",
        userSelect: "none",
      }}
    >
      {children}
    </div>
  );
}

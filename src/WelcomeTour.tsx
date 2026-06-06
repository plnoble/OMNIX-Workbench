import React, { useState, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";

export interface TourStep {
  targetId?: string;
  tab: string;
  title: string;
  content: string;
  position?: "right" | "bottom" | "top" | "left" | "center";
}

const TOUR_STEPS: TourStep[] = [
  {
    tab: "dashboard",
    title: "🔮 OMNIX DevFlow 交互式开发中枢",
    content: "欢迎来到 OMNIX DevFlow！这是为您量身打造的开发提效网关。我们特别准备了 6 步简易指引，带您领略包括智能大模型路由切换、技能熔炼拓扑与开发经验事故蒸馏在内的全部高级特性。",
    position: "center"
  },
  {
    targetId: "nav-dashboard",
    tab: "dashboard",
    title: "📊 灵动网关控制面板",
    content: "控制面板是您的运营看板。这里能查看到本地 API 代理的运行健康状况，并在右侧进行 Cron 任务定时唤醒管理，以及轮播查阅有助于避坑提效的研发贴士。",
    position: "right"
  },
  {
    targetId: "nav-agents",
    tab: "agents",
    title: "🤖 Agent 仓库与多账户热切",
    content: "在这里可以扫描和一键沙箱部署 CLI Agent。上方卡槽集成了零中断多账号热切换面板，让您在开发时随心切换各种 API，更可以通过输入“Auto”启用智能自动路由切换！",
    position: "right"
  },
  {
    targetId: "nav-skills",
    tab: "skills",
    title: "🧬 自进化技能拓扑融合炉",
    content: "这是 OMNIX 的核心进化设施！在这里，您能用 D3.js 物理力学拓扑查看技能之间的双链依赖；还可以在卡槽中多选不同的规则包进行‘Ignite 融合’，智能生成融合后的超级技能卡片。",
    position: "right"
  },
  {
    targetId: "nav-memories",
    tab: "memories",
    title: "🧠 长期防错事故避坑记忆",
    content: "智能预防‘二次事故’！系统会在启动 Agent 时，自动为您的工作区注入 CLAUDE.md 事故避坑规则。当开发 Timeline 结束后，您可以一键‘经验蒸馏’提炼新记忆卡片，沉淀为资产。",
    position: "right"
  },
  {
    targetId: "nav-team",
    tab: "team",
    title: "👥 Team Mode 协同控制台",
    content: "真正的团队工作空间。左侧内置了实时双向 Standard Input/Output 进程终端控制台；右侧配备了层级折叠树状计划任务组件，让 Teammate Agent 之间的异步分工一目了然。",
    position: "right"
  },
  {
    targetId: "nav-settings",
    tab: "settings",
    title: "⚙️ 中转网关与系统参数",
    content: "在这里配置代理监听端口、一键将 API 凭证静默同步给本地各种 Agent，或开启 WSL 虚拟机集成。如果您在模型名中快捷选择或输入了“Auto”，网关将根据当前请求意图智能自动调配合适的大模型！",
    position: "right"
  }
];

interface WelcomeTourProps {
  activeTab: string;
  setActiveTab: (tab: string) => void;
  onClose: () => void;
}

export const WelcomeTour: React.FC<WelcomeTourProps> = ({
  activeTab,
  setActiveTab,
  onClose,
}) => {
  const [currentStep, setCurrentStep] = useState(0);
  const [coords, setCoords] = useState<{
    top: number;
    left: number;
    width: number;
    height: number;
  } | null>(null);
  
  const step = TOUR_STEPS[currentStep];
  const resizeRef = useRef<number | null>(null);

  // Sync state tab with tour tab step
  useEffect(() => {
    if (step.tab && step.tab !== activeTab) {
      setActiveTab(step.tab);
    }
  }, [currentStep, step.tab]);

  // Recalculate target positions
  const updatePosition = () => {
    if (!step.targetId) {
      setCoords(null);
      return;
    }

    const element = document.getElementById(step.targetId);
    if (element) {
      const rect = element.getBoundingClientRect();
      setCoords({
        top: rect.top,
        left: rect.left,
        width: rect.width,
        height: rect.height,
      });
    } else {
      // Element might not be rendered yet, retry in a moment
      setTimeout(() => {
        const el = document.getElementById(step.targetId!);
        if (el) {
          const rect = el.getBoundingClientRect();
          setCoords({
            top: rect.top,
            left: rect.left,
            width: rect.width,
            height: rect.height,
          });
        }
      }, 150);
    }
  };

  useEffect(() => {
    // Wait slightly for tab changes animations
    const timer = setTimeout(updatePosition, 200);
    
    const handleResize = () => {
      if (resizeRef.current) cancelAnimationFrame(resizeRef.current);
      resizeRef.current = requestAnimationFrame(updatePosition);
    };

    window.addEventListener("resize", handleResize);
    return () => {
      clearTimeout(timer);
      window.removeEventListener("resize", handleResize);
      if (resizeRef.current) cancelAnimationFrame(resizeRef.current);
    };
  }, [currentStep, activeTab]);

  const handleNext = () => {
    if (currentStep < TOUR_STEPS.length - 1) {
      setCurrentStep(prev => prev + 1);
    } else {
      handleComplete();
    }
  };

  const handleBack = () => {
    if (currentStep > 0) {
      setCurrentStep(prev => prev - 1);
    }
  };

  const handleComplete = async () => {
    try {
      await invoke("set_app_setting", { key: "onboarding_completed", value: "true" });
    } catch (e) {
      console.error("Failed to save onboarding state to DB:", e);
    }
    onClose();
  };

  // Target coordinates mapping for Popover placement
  const getPopoverStyle = (): React.CSSProperties => {
    if (!coords) {
      return {
        position: "fixed",
        top: "50%",
        left: "50%",
        transform: "translate(-50%, -50%)",
        width: "420px",
        zIndex: 10001,
      };
    }

    const spacing = 18;
    const popoverWidth = 320;

    switch (step.position) {
      case "right":
        return {
          position: "absolute",
          top: coords.top + coords.height / 2 - 120, // offset half height
          left: coords.left + coords.width + spacing,
          width: `${popoverWidth}px`,
          zIndex: 10001,
        };
      case "bottom":
        return {
          position: "absolute",
          top: coords.top + coords.height + spacing,
          left: coords.left + coords.width / 2 - popoverWidth / 2,
          width: `${popoverWidth}px`,
          zIndex: 10001,
        };
      case "top":
        return {
          position: "absolute",
          top: coords.top - spacing - 240, // assume popover height 240px
          left: coords.left + coords.width / 2 - popoverWidth / 2,
          width: `${popoverWidth}px`,
          zIndex: 10001,
        };
      case "left":
        return {
          position: "absolute",
          top: coords.top + coords.height / 2 - 120,
          left: coords.left - popoverWidth - spacing,
          width: `${popoverWidth}px`,
          zIndex: 10001,
        };
      default:
        return {
          position: "fixed",
          top: "50%",
          left: "50%",
          transform: "translate(-50%, -50%)",
          width: "420px",
          zIndex: 10001,
        };
    }
  };

  return (
    <div className="tour-overlay">
      {/* Target Element Highlight Ring */}
      {coords && (
        <div 
          className="tour-highlight-ring"
          style={{
            top: coords.top - 6,
            left: coords.left - 6,
            width: coords.width + 12,
            height: coords.height + 12,
          }}
        />
      )}

      {/* Onboarding Popover Card */}
      <div className="tour-popover" style={getPopoverStyle()}>
        <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: "12px" }}>
          <h4 className="tour-title">{step.title}</h4>
          <span className="tour-steps-count">
            {currentStep + 1} / {TOUR_STEPS.length}
          </span>
        </div>
        
        <p className="tour-content">{step.content}</p>
        
        <div className="tour-actions" style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginTop: "16px" }}>
          <button 
            className="tour-btn-skip" 
            onClick={handleComplete}
            title="跳过并在此后默认为已完成"
          >
            跳过指引
          </button>
          
          <div style={{ display: "flex", gap: "8px" }}>
            {currentStep > 0 && (
              <button className="tour-btn tour-btn-secondary" onClick={handleBack}>
                上一步
              </button>
            )}
            <button className="tour-btn tour-btn-primary" onClick={handleNext}>
              {currentStep === TOUR_STEPS.length - 1 ? "完成指引" : "下一步"}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
};

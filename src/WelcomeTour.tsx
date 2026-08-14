import React, { useState, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { PRODUCT_DESCRIPTOR_ZH, PRODUCT_NAME } from "@/lib/constants";

export interface TourStep {
  targetId?: string;
  tab: string;
  title: string;
  content: string;
  position?: "right" | "bottom" | "top" | "left" | "center";
}

/**
 * 首次引导。
 *
 * 上一版整段是过期的：第一步跳 `dashboard`（那个页已并进设置›诊断，
 * `handleTabChange` 会把它重写掉），每一步的 `targetId` 找的是 `nav-*` 元素而
 * DOM 里**一个都没有**，文案还在讲「灵动网关控制面板」「Ignite 融合」
 * 「stdin/stdout 团队控制台」和 WSL 集成——后者已经删掉了。
 *
 * 新用户第一次被带着走的，不该是一条已拆除的路。现在按**主路径**排：
 * 对话 → 工作 → 模型 → 技能 → 其余。去掉了 `targetId`——没有可高亮的锚点就
 * 别假装有，居中说明反而更清楚。
 */
const TOUR_STEPS: TourStep[] = [
  {
    tab: "chat",
    title: `欢迎使用 ${PRODUCT_NAME}`,
    content: `${PRODUCT_DESCRIPTOR_ZH}。四步就能上手——先认识主路径，其余功能都在宫格里，用到再看。`,
    position: "center"
  },
  {
    tab: "chat",
    title: "① 对话：选一个 Agent，直接说话",
    content: "顶部选 Claude Code / Codex / Gemini 等已安装的 Agent，输入框直接提问。输入 /goal 可以给这条对话钉一个长期目标，之后每一轮都会朝它推进。",
    position: "center"
  },
  {
    tab: "work",
    title: "② 工作：把 Agent 接到一个目录上",
    content: "选一个工作区，Agent 就能读写那里的文件、跑命令、开工作树。左侧能看到改动、检查点和子代理，随时回滚。",
    position: "center"
  },
  {
    tab: "models",
    title: "③ 模型中心：配平台和 Key",
    content: "加平台、填 API Key（多把 Key 可以轮换和故障切换）、拉取模型列表。模型名选「Auto」时，网关会按这次请求需要的能力自动挑一个。注意：走自家协议的 Agent（Gemini / Qwen / OpenCode / Copilot / Grok）用的是它们自己的模型配置，这里的设置对它们不生效。",
    position: "center"
  },
  {
    tab: "skills",
    title: "④ 技能：把 SKILL.md 交给所有 Agent",
    content: "导入或新建技能，晋升到正式池之后，所有走网关的 Agent 都能直接调用；也可以同步成文件分发给不走网关的工具。其余功能（知识库、办公、定时任务、监控…）都在左上角宫格里。",
    position: "center"
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
  const retryTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

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
      if (retryTimerRef.current) clearTimeout(retryTimerRef.current);
      retryTimerRef.current = setTimeout(() => {
        if (!step.targetId) return;
        const el = document.getElementById(step.targetId);
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
      if (retryTimerRef.current) clearTimeout(retryTimerRef.current);
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
        <div className="flex items-center justify-between mb-3">
          <h4 className="tour-title">{step.title}</h4>
          <span className="tour-steps-count">
            {currentStep + 1} / {TOUR_STEPS.length}
          </span>
        </div>

        <p className="tour-content">{step.content}</p>

        <div className="tour-actions flex items-center justify-between mt-4">
          <button
            className="tour-btn-skip"
            onClick={handleComplete}
            title="跳过并在此后默认为已完成"
          >
            跳过指引
          </button>

          <div className="flex gap-2">
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

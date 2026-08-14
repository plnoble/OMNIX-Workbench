/**
 * 定时拉数据，**窗口看不见时停下**。
 *
 * 收拢之前，这个模式在 8 个组件里各写了一遍：
 *
 * ```ts
 * useEffect(() => {
 *   void load();
 *   const t = setInterval(() => void load(), 10_000);
 *   return () => clearInterval(t);
 * }, [load]);
 * ```
 *
 * 八份都没管窗口是不是还看得见。OMNIX 有「启动即最小化到托盘」这个设置，托盘
 * 里挂一整天是常态用法——那期间这些定时器一个不落地跑着：网关健康、订阅额度、
 * 监督审批、Team 详情、远程设备……全在给一个没人看的界面拉数据。手机配对那条
 * 更直接：它每 4 分半**发一个新的一次性配对码**，屏幕根本不亮着。
 *
 * 现在只有这一处需要管这件事。真正在跑的那类后台工作（`useAutopilotRunner`）
 * **故意不走这条**：自动任务是「不看着也要按时触发」的，停掉就是把功能关了。
 */

import { useEffect, useRef } from "react";

/** 可见性的来源。抽成接口是为了能在 node 环境里测，浏览器里就是 `document`。 */
export interface VisibilitySource {
  isHidden: () => boolean;
  /** 订阅变化，返回退订函数。 */
  subscribe: (handler: () => void) => () => void;
}

export const documentVisibility: VisibilitySource = {
  isHidden: () => typeof document !== "undefined" && document.hidden,
  subscribe: (handler) => {
    if (typeof document === "undefined") return () => {};
    document.addEventListener("visibilitychange", handler);
    return () => document.removeEventListener("visibilitychange", handler);
  },
};

/**
 * 可见时按 `intervalMs` 反复调用 `run`，隐藏时停，**重新可见时立刻补一次**。
 *
 * 补这一次是必需的：回到界面时用户看到的必须是现在的状态，不能是被暂停那一刻
 * 的旧值，更不能干等一个完整周期。
 *
 * 返回清理函数。
 */
export function pollWhileVisible(
  run: () => void,
  intervalMs: number,
  source: VisibilitySource = documentVisibility,
): () => void {
  let timer: ReturnType<typeof setInterval> | null = null;

  const start = () => {
    if (timer !== null) return;
    run();
    timer = setInterval(run, intervalMs);
  };
  const stop = () => {
    if (timer === null) return;
    clearInterval(timer);
    timer = null;
  };

  if (!source.isHidden()) start();
  const unsubscribe = source.subscribe(() => {
    if (source.isHidden()) stop();
    else start();
  });

  return () => {
    unsubscribe();
    stop();
  };
}

/**
 * `pollWhileVisible` 的 React 包装。
 *
 * `load` 存在 ref 里，所以**调用方不用把它 useCallback 稳住**——换了一个新的
 * 闭包不会重建定时器，但下一拍用的一定是最新那个。以前几处写成 `[load]` 依赖，
 * 只要上游忘了 useCallback 就会每次渲染重开一个定时器。
 *
 * `enabled` 为 false 时整个不启动（例如远程访问没开时就别去查设备列表）。
 */
export function usePolling(
  load: () => void | Promise<void>,
  intervalMs: number,
  enabled = true,
  source: VisibilitySource = documentVisibility,
): void {
  const latest = useRef(load);
  latest.current = load;

  useEffect(() => {
    if (!enabled || intervalMs <= 0) return;
    return pollWhileVisible(() => void latest.current(), intervalMs, source);
  }, [intervalMs, enabled, source]);
}

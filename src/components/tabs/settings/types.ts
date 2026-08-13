/**
 * 设置页的壳还需要的东西——只剩「当前在哪个子页」和诊断面板节点。
 *
 * 这里以前是一个 ~85 个字段的大接口：平台、系统表单、划词、翻译、主题、MCP、
 * 备份全塞在一起，由 App.tsx 一次性传给 `SettingsTab` / `McpTab`，再靠
 * `{...props}` 往四个子页扩散。现在四个子页各自从 store 取自己那个域。
 *
 * 顺带清掉了划词（selectionCaptureMode / selectionHistory / …）和翻译
 * （translatePreferredLang / translateModel / …）那两整块约 25 个字段——
 * 它们在设置的任何一个子组件里都**零引用**，是划词助手和翻译各自独立成页之后
 * 留下来的，App.tsx 还在老老实实地传。
 */
import type { SettingsSubTab } from "@/types";

export interface SettingsTabProps {
  settingsSubTab: SettingsSubTab;
  setSettingsSubTab: (tab: SettingsSubTab) => void;
  /** 整块诊断面板由 App 构造后作为节点传入；专页复用（如 McpTab）不提供。 */
  diagnosticsPanel?: React.ReactNode;
}

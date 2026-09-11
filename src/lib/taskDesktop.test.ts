import { describe, expect, it } from "vitest";

import {
  applyPanelRequest,
  applyReview,
  closePanel,
  emptyEvidence,
  emptyTaskLayout,
  evidenceFromAgentRun,
  evidenceShelfRows,
  parseStoredLayout,
  pickRecentWork,
  recordVerification,
  restoredProcessLabel,
  reviewDoesNotFollowVerification,
  upsertArtifact,
  verificationForArtifact,
  type TaskArtifact,
  type TaskVerification,
} from "./taskDesktop";

describe("pickRecentWork", () => {
  const list = [
    { id: "c1", title: "闲聊", workspace_path: "direct", active_agent: "claude", created_at: "2026-09-02T00:00:00Z" },
    { id: "w1", title: "旧项目", workspace_path: "D:/old", active_agent: "claude", created_at: "2026-09-08T00:00:00Z" },
    { id: "w2", title: "新项目", workspace_path: "D:/new", active_agent: "claude", created_at: "2026-09-10T00:00:00Z" },
    { id: "w3", title: "别人的", workspace_path: "D:/other", active_agent: "codex", created_at: "2026-09-11T00:00:00Z" },
  ];

  it("工作首屏恢复最近一条工作会话，不要求先布置空桌面", () => {
    expect(pickRecentWork(list, "claude")).toEqual({
      conversationId: "w2",
      title: "新项目",
      workspacePath: "D:/new",
      agent: "claude",
      createdAt: "2026-09-10T00:00:00Z",
    });
  });

  it("普通对话不算最近工作", () => {
    expect(pickRecentWork(list.filter((item) => item.id === "c1"))).toBeNull();
  });
});

describe("task panels", () => {
  it("Agent 打开已有面板时去重，默认不抢焦点", () => {
    const layout = emptyTaskLayout("conv1", "D:/repo", "conv1");
    const opened = applyPanelRequest(layout, {
      id: "preview:README.md",
      kind: "preview",
      taskId: "conv1",
      resource: "README.md",
    });
    const again = applyPanelRequest(opened, {
      id: "preview:other",
      kind: "preview",
      taskId: "conv1",
      resource: "README.md",
    });
    expect(again.panels).toHaveLength(1);
    expect(again.panels[0].focused).toBe(false);
  });

  it("关闭面板不等于终止后台任务", () => {
    const layout = applyPanelRequest(emptyTaskLayout("conv1", "D:/repo", "conv1"), {
      id: "logs:run",
      kind: "logs",
      taskId: "conv1",
      resource: "session:conv1",
      stealFocus: true,
    });
    const closed = closePanel(layout, "logs:run");
    expect(closed.panels).toHaveLength(0);
    expect(restoredProcessLabel()).toBe("已结束");
  });

  it("恢复布局时去掉焦点，避免重启后抢用户当前操作", () => {
    const stored = JSON.stringify({
      version: 1,
      taskId: "conv1",
      workspacePath: "D:/repo",
      conversationId: "conv1",
      panels: [{ id: "preview:a", kind: "preview", taskId: "conv1", resource: "a.md", focused: true }],
    });
    const restored = parseStoredLayout(stored, "conv1");
    expect(restored?.panels[0].focused).toBe(false);
    expect(parseStoredLayout(stored, "conv2")).toBeNull();
  });
});

describe("artifacts and verification", () => {
  const artifact: TaskArtifact = {
    id: "art1",
    taskId: "conv1",
    path: "src/lib.ts",
    version: "v2",
    hash: "abc",
    source: { agent: "claude", stepId: "step-3" },
  };

  it("产物版本变化后旧验证失效，测试通过不等于用户接受", () => {
    const verification: TaskVerification = {
      id: "ver1",
      artifactId: "art1",
      artifactVersion: "v1",
      command: "npm test",
      exitCode: 0,
      report: "ok",
      status: "passed",
    };
    expect(verificationForArtifact(artifact, verification).status).toBe("stale");
    expect(reviewDoesNotFollowVerification(
      { ...verification, artifactVersion: "v2", status: "passed" },
      undefined,
    )).toBe(true);
  });

  it("复用 agent_run 字段，测试通过不会自动写成用户接受", () => {
    const mapped = evidenceFromAgentRun({
      id: "ar1",
      run_id: "run1",
      agent_name: "claude",
      task_title: "src/lib.ts",
      result_summary: "npm test ok",
      validation_status: "passed",
    });
    let bundle = emptyEvidence("conv1");
    bundle = recordVerification(upsertArtifact(bundle, mapped.artifact), mapped.verification);
    const rows = evidenceShelfRows(bundle);
    expect(rows).toHaveLength(1);
    expect(rows[0].verification?.status).toBe("passed");
    expect(rows[0].acceptedByTestsOnly).toBe(true);
    bundle = applyReview(bundle, { artifactId: mapped.artifact.id, decision: "accepted" });
    expect(evidenceShelfRows(bundle)[0].acceptedByTestsOnly).toBe(false);
  });
});

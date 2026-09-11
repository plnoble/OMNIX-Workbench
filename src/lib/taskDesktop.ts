/**
 * Task desktop — layout, evidence, and resume helpers.
 *
 * Reuses conversation / workspace / run identity. Do not invent a parallel
 * task system. Restoring a layout restores panel descriptions and resource
 * locators, never live processes.
 */

export const TASK_DESKTOP_LAYOUT_VERSION = 1 as const;

export type TaskPanelKind = "files" | "diff" | "logs" | "preview" | "tests";

export interface TaskPanelState {
  id: string;
  kind: TaskPanelKind;
  /** Conversation / workspace / run this panel belongs to. */
  taskId: string;
  resource: string;
  /** Opened in the background unless the user must decide. */
  focused: boolean;
}

export interface TaskDesktopLayout {
  version: typeof TASK_DESKTOP_LAYOUT_VERSION;
  taskId: string;
  workspacePath: string;
  conversationId: string;
  panels: TaskPanelState[];
}

export type ArtifactSource = {
  agent: string;
  stepId: string;
};

export interface TaskArtifact {
  id: string;
  taskId: string;
  path: string;
  version: string;
  hash: string;
  source: ArtifactSource;
}

export type VerificationStatus = "passed" | "failed" | "stale";

export interface TaskVerification {
  id: string;
  artifactId: string;
  artifactVersion: string;
  command: string;
  exitCode: number;
  report: string;
  status: VerificationStatus;
}

export type ReviewDecision = "accepted" | "rejected";

export interface TaskReview {
  artifactId: string;
  decision: ReviewDecision;
}

export interface RecentWorkItem {
  conversationId: string;
  title: string;
  workspacePath: string;
  agent: string;
  createdAt: string;
}

export function emptyTaskLayout(taskId: string, workspacePath: string, conversationId: string): TaskDesktopLayout {
  return {
    version: TASK_DESKTOP_LAYOUT_VERSION,
    taskId,
    workspacePath,
    conversationId,
    panels: [],
  };
}

export function isLiveProcessPanel(_kind: TaskPanelKind): boolean {
  return false;
}

/** Restored terminals/processes are descriptions, never auto-rerun. */
export function restoredProcessLabel(): string {
  return "已结束";
}

export function applyPanelRequest(
  layout: TaskDesktopLayout,
  request: Omit<TaskPanelState, "focused"> & { stealFocus?: boolean },
): TaskDesktopLayout {
  const existing = layout.panels.find((panel) => panel.id === request.id || (
    panel.kind === request.kind && panel.resource === request.resource
  ));
  const nextPanel: TaskPanelState = {
    id: existing?.id ?? request.id,
    kind: request.kind,
    taskId: request.taskId,
    resource: request.resource,
    focused: request.stealFocus === true,
  };
  const others = layout.panels
    .filter((panel) => panel.id !== nextPanel.id)
    .map((panel) => (nextPanel.focused ? { ...panel, focused: false } : panel));
  return { ...layout, panels: [...others, nextPanel] };
}

export function closePanel(layout: TaskDesktopLayout, panelId: string): TaskDesktopLayout {
  return {
    ...layout,
    panels: layout.panels.filter((panel) => panel.id !== panelId),
  };
}

export function verificationForArtifact(
  artifact: TaskArtifact,
  verification: TaskVerification,
): TaskVerification {
  if (verification.artifactId !== artifact.id || verification.artifactVersion !== artifact.version) {
    return { ...verification, status: "stale" };
  }
  return verification;
}

export function reviewDoesNotFollowVerification(
  verification: TaskVerification | undefined,
  review: TaskReview | undefined,
): boolean {
  return review?.decision !== "accepted" || verification?.status !== "passed";
}

export function pickRecentWork(
  conversations: Array<{
    id: string;
    title: string;
    workspace_path?: string | null;
    active_agent: string;
    created_at: string;
  }>,
  agent?: string,
): RecentWorkItem | null {
  const candidates = conversations
    .filter((conv) => !!conv.workspace_path && conv.workspace_path !== "direct")
    .filter((conv) => !agent || conv.active_agent === agent)
    .sort((a, b) => b.created_at.localeCompare(a.created_at));
  const conv = candidates[0];
  if (!conv || !conv.workspace_path) return null;
  return {
    conversationId: conv.id,
    title: conv.title,
    workspacePath: conv.workspace_path,
    agent: conv.active_agent,
    createdAt: conv.created_at,
  };
}

const LAYOUT_KEY_PREFIX = "omnix_task_desktop_layout_";
const EVIDENCE_KEY_PREFIX = "omnix_task_desktop_evidence_";

export function layoutStorageKey(taskId: string): string {
  return `${LAYOUT_KEY_PREFIX}${taskId}`;
}

export function evidenceStorageKey(taskId: string): string {
  return `${EVIDENCE_KEY_PREFIX}${taskId}`;
}

export function parseStoredLayout(raw: string | null, expectedTaskId: string): TaskDesktopLayout | null {
  if (!raw) return null;
  try {
    const parsed = JSON.parse(raw) as TaskDesktopLayout;
    if (parsed.version !== TASK_DESKTOP_LAYOUT_VERSION) return null;
    if (parsed.taskId !== expectedTaskId) return null;
    if (!Array.isArray(parsed.panels)) return null;
    return {
      ...parsed,
      panels: parsed.panels.map((panel) => ({ ...panel, focused: false })),
    };
  } catch {
    return null;
  }
}

export interface TaskEvidenceBundle {
  taskId: string;
  artifacts: TaskArtifact[];
  verifications: TaskVerification[];
  reviews: TaskReview[];
}

export function emptyEvidence(taskId: string): TaskEvidenceBundle {
  return { taskId, artifacts: [], verifications: [], reviews: [] };
}

export function parseStoredEvidence(raw: string | null, expectedTaskId: string): TaskEvidenceBundle | null {
  if (!raw) return null;
  try {
    const parsed = JSON.parse(raw) as TaskEvidenceBundle;
    if (parsed.taskId !== expectedTaskId) return null;
    if (!Array.isArray(parsed.artifacts) || !Array.isArray(parsed.verifications) || !Array.isArray(parsed.reviews)) {
      return null;
    }
    return parsed;
  } catch {
    return null;
  }
}

export function upsertArtifact(bundle: TaskEvidenceBundle, artifact: TaskArtifact): TaskEvidenceBundle {
  const rest = bundle.artifacts.filter((item) => item.id !== artifact.id && item.path !== artifact.path);
  const previous = bundle.artifacts.find((item) => item.id === artifact.id || item.path === artifact.path);
  const nextArtifact = previous && previous.hash !== artifact.hash
    ? { ...artifact, id: previous.id, version: artifact.version }
    : { ...artifact, id: previous?.id ?? artifact.id };
  const verifications = bundle.verifications.map((item) => (
    item.artifactId === nextArtifact.id ? verificationForArtifact(nextArtifact, item) : item
  ));
  return { ...bundle, artifacts: [...rest, nextArtifact], verifications };
}

export function recordVerification(bundle: TaskEvidenceBundle, verification: TaskVerification): TaskEvidenceBundle {
  const artifact = bundle.artifacts.find((item) => item.id === verification.artifactId);
  const next = artifact ? verificationForArtifact(artifact, verification) : { ...verification, status: "stale" as const };
  return {
    ...bundle,
    verifications: [...bundle.verifications.filter((item) => item.id !== next.id), next],
  };
}

export function applyReview(bundle: TaskEvidenceBundle, review: TaskReview): TaskEvidenceBundle {
  return {
    ...bundle,
    reviews: [...bundle.reviews.filter((item) => item.artifactId !== review.artifactId), review],
  };
}

/** Map an existing agent_run onto evidence without inventing a second task id. */
export function evidenceFromAgentRun(run: {
  id: string;
  run_id: string;
  agent_name: string;
  task_title: string;
  result_summary: string;
  validation_status: string;
  log_excerpt?: string;
}): { artifact: TaskArtifact; verification: TaskVerification } {
  const artifact: TaskArtifact = {
    id: `run:${run.id}`,
    taskId: run.run_id,
    path: run.task_title,
    version: run.id,
    hash: run.id,
    source: { agent: run.agent_name, stepId: run.id },
  };
  const passed = run.validation_status === "passed" || run.validation_status === "completed";
  const failed = run.validation_status === "failed" || run.validation_status === "validation_failed";
  const verification: TaskVerification = {
    id: `ver:${run.id}`,
    artifactId: artifact.id,
    artifactVersion: artifact.version,
    command: run.task_title,
    exitCode: failed ? 1 : passed ? 0 : -1,
    report: run.result_summary || run.log_excerpt || "",
    status: passed ? "passed" : failed ? "failed" : "stale",
  };
  return { artifact, verification };
}

export interface EvidenceShelfRow {
  artifact: TaskArtifact;
  verification?: TaskVerification;
  review?: TaskReview;
  acceptedByTestsOnly: boolean;
}

export function evidenceShelfRows(bundle: TaskEvidenceBundle): EvidenceShelfRow[] {
  return bundle.artifacts.map((artifact) => {
    const verification = bundle.verifications.find((item) => item.artifactId === artifact.id);
    const review = bundle.reviews.find((item) => item.artifactId === artifact.id);
    const bound = verification ? verificationForArtifact(artifact, verification) : undefined;
    return {
      artifact,
      verification: bound,
      review,
      acceptedByTestsOnly: bound?.status === "passed" && review?.decision !== "accepted",
    };
  });
}

export function panelKindLabel(kind: TaskPanelKind): string {
  switch (kind) {
    case "files": return "文件";
    case "diff": return "Diff";
    case "logs": return "日志";
    case "preview": return "预览";
    case "tests": return "测试报告";
    default: return kind;
  }
}

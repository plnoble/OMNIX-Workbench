---
name: agent-dev-protocol
description: Follow the repository's multi-agent development protocol for planning, implementation, quality gates, verification, handoff, reflection, error recording, and experience distillation. Use when working on development tasks, bug fixes, refactors, multi-agent handoffs, implementation plans, pre-change or pre-delivery checks, verification, retrospectives, or reusable lesson capture.
---

# Agent Development Protocol

Follow the shared project protocol before and during development work.

## Startup

1. Read `AGENTS.md`.
2. Read `.omx/development/current.md`.
3. Read `.omx/development/handoff.md`.
4. Inspect the current worktree before trusting conversation history.

## Work Loop

1. State or update the current objective.
2. Plan the next verifiable step.
3. Implement in small, reversible chunks.
4. Verify with the narrowest meaningful command.
5. Record important decisions, errors, risks, and next steps.
6. Run applicable quality gates and add a short reflection before completing meaningful work.
7. Update handoff before stopping.

## Required Updates

- Update `.omx/development/current.md` when status, blockers, verification, or next action changes.
- Append to `.omx/development/worklog.md` for meaningful milestones.
- Append to `.omx/development/decisions.md` when choosing between plausible approaches.
- Append to `.omx/development/error-ledger.md` when an assumption or implementation path fails.
- Append to `.omx/development/reflections.md` before declaring a meaningful task finished.
- Use `.omx/development/quality-gates.md` for risky changes, handoff, delivery, or release.
- Append to `.omx/development/skill-candidates.md` for reusable lessons.
- Update `.omx/development/handoff.md` before ending the session.

## Error Handling

When a mistake happens, record the symptom, wrong assumption, root cause, detection method, fix, prevention rule, and whether it should become a skill candidate.

## Completion

Before reporting completion, verify the requested behavior, record applicable quality-gate results, add a reflection, update handoff, and review whether any project rule, script, template, or skill should be created from the work.
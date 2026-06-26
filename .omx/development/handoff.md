# Agent Handoff

## Current Objective

User acceptance is underway. Three items have landed in the worktree since the release candidate (all gate-verified, not yet packaged): the Codex `thread/start` timeout fix, per-Agent chat isolation, and roadmap **P1** — Codex/Claude default model now reaches the runtime via a Responses↔Chat translating gateway. Next is user acceptance of these, then roadmap P2–P5.

## Latest Changes (2026-06-25)

- `responses_bridge.rs` translates Codex's Responses API to/from Chat Completions; `proxy.rs handle_responses_for_session` forwards native Responses providers verbatim and translates Chat providers. Validated end-to-end against the real `codex app-server`.
- Model resolution: Agent binding → global `default_model` setting → Agent default; the Work page pre-selects the resolved default (`is_default`, fixed to re-derive on Agent switch). `evaluate_model_compatibility` now allows Chat providers for Codex (gateway-translated).
- P2 model center: global ☆ "Agent 默认" model + ZCF provider presets in `PlatformModal`; capability icons + health checks already existed. `target_model` (Settings → "内置功能默认模型") vs `default_model` (Models ☆) are distinct by design — see decisions.
- UX: 对话/工作 split (Work requires a workspace), tab-chunk preload, first-token waiting indicator, portaled delete dialog.
- The user must re-test online with their own provider key (set a Chat-Completions provider as the Agent default and confirm Codex uses it). Reference repos are in gitignored `scratch/`.
- Next: P3 (MCP one-config sync), P4 (Team board on `aionui.rs` mailbox/task-DAG backend), P5 (assistants/Skills/remote) — detailed plans requested before implementation. A fresh Windows package must be rebuilt before redistribution (current Desktop artifacts predate P1/P2).

## (Prior) Objective

The full OMNIX Workbench real-gap release candidate is complete. The next task is user acceptance testing and feedback collection, not another hidden implementation pass.

## Implemented

- Claude Code stream-JSON and Codex app-server structured runtime adapters.
- System-first CLI detection, official isolated managed installs, truthful runtime support states, and protocol-specific model compatibility.
- Persisted Agent sessions, chat/tool/approval/error messages, raw runtime events, stop, resume, and restart restoration.
- Session model precedence, direct/plan modes, per-session full-access confirmation, primary API Key plus failover, and real workspace/Git state.
- User-approved Team manager plans, dependency/concurrency scheduling, Worker approvals, retries, cancellation, persistence, and final manager validation.
- Real Git-backed Skill preview/import with provenance, model-generated fusion drafts, split conflict review, approval, Skill Sets, and truthful sync verification.
- Evidence-backed memory, Skill, and protocol distillation inbox; no automatic write without approval.
- Named Knowledge bases, multi-base ordinary-chat RAG, citation metadata, native file/folder selection, and conservative Quick Assistant capture with blacklists.
- Focused Work-first shell, configurable top navigation/app grid, context-only sidebars, stable title-bar tools, responsive workspace overlay, and short-height welcome state.
- OMNIX Workbench naming contract, provisional E1 branding, stable MSI upgrade identity, and Windows MSI/NSIS/release artifacts.

## Final Verification

- `npx.cmd tsc --noEmit`: passed.
- `npm.cmd run build`: passed.
- `cargo check`: passed with 90 known legacy warnings, down from the 126-warning baseline.
- `cargo test --lib`: 70 passed, 0 failed, 4 ignored.
- `npm.cmd run tauri -- build`: passed; normal WiX MSI validation and NSIS generation completed.
- Release exe launched successfully.
- Per-Monitor-V2 native visual QA passed for the full 2586x1655 physical window and the 1280x1040 physical / 640x520 logical minimum window.
- No OMNIX Workbench process was intentionally left running after QA.

## Artifacts

- `src-tauri/target/release/omnix-workbench.exe`
  - SHA256: `1DBBA2848082A1A0E339CD98A06D24670C481161B54E38EDE9F717441DE961DE`
- `src-tauri/target/release/bundle/msi/OMNIX Workbench_0.1.0_x64_en-US.msi`
  - SHA256: `2E92C98E591DF8D225B9B7654B78CCF1D08D680A3028B9A7AE856FED6C64CC52`
- `src-tauri/target/release/bundle/nsis/OMNIX Workbench_0.1.0_x64-setup.exe`
  - SHA256: `6949F3A797BE84472ACDD851552E4F4C7878AD062FC29B1004C4B591AD8EF8B2`
- Desktop delivery folder: `OMNIX Workbench 0.1.0 测试版`.
- Documentation: complete detection plan, user manual, and 0.1.0 release manifest.

## Explicit Boundaries

- The first supported structured runtime milestone covers Claude Code and Codex. Gemini CLI, OpenCode, and other Agents remain marked as pending until their structured adapters are verified.
- Goal pursuit, unverified cross-tool Skill sync, and incomplete cross-platform Quick Assistant behavior stay in Labs.
- Automated runtime tests do not spend a real online model request. User acceptance must exercise the user's own CLI login/API keys and network.
- Remaining Rust warnings are technical debt, not failed verification.

## Next Acceptance Flow

1. Install from the Desktop MSI or NSIS package.
2. Add/enable a model provider and perform health checks.
3. Detect Claude Code and Codex; verify default, official/builtin, and OMNIX model bindings.
4. Run one ordinary chat and one workspace task with each supported Agent, including approval, stop, app restart, and history restoration.
5. Approve a Team plan and inspect Worker dependency/retry behavior.
6. Exercise Skill import/fusion approval, distillation inbox approval, named Knowledge citations, and Quick Assistant blacklist behavior.
7. Record user feedback before the next reference-software borrowing slice.

## Safety Notes

- Do not log decrypted API keys or persist full-access confirmation globally.
- Do not reintroduce terminal-text parsing as structured Agent output or mock success states.
- Do not distribute older bundle files as this release.
- Native Windows screenshot tools must declare Per-Monitor-V2 DPI awareness before measuring or capturing windows.
- Preserve existing SQLite data, stable WiX upgrade code, naming contract, and E1 design history.

# Feishu Group Binding Uniqueness Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Enforce one active Vibe Kanban workspace binding per Feishu group and show a clear conflict message during binding.

**Architecture:** Add service-layer validation that resolves bindings by Feishu group identity rather than by `bot_id + target_id` alone. Keep dispatcher self-healing only for legacy duplicate rows on the exact same target and surface binding conflicts in the workspace binding dialog.

**Tech Stack:** Rust (`sqlx`, `axum`), React/TypeScript (`web-core`, React Query), Mintlify/MD docs.

---

### Task 1: Add failing backend tests for group-level uniqueness

**Files:**
- Modify: `crates/server/src/feishu/service.rs`

**Step 1: Write the failing test**
- Add a test proving that two different bot targets that resolve to the same Feishu group cannot both be actively bound to different workspaces.

**Step 2: Run test to verify it fails**
- Run: `DATABASE_URL=sqlite://$(pwd)/dev_assets/db.v2.sqlite cargo test -p server create_binding_rejects_group_already_bound_to_another_workspace -- --nocapture`
- Expected: FAIL because the previous logic only enforced uniqueness per bot target.

**Step 3: Write minimal implementation**
- Add group-identity lookup and binding conflict detection in the Feishu service.

**Step 4: Run test to verify it passes**
- Re-run the same command and expect PASS.

### Task 2: Add failing backend tests for re-enable conflicts

**Files:**
- Modify: `crates/server/src/feishu/service.rs`

**Step 1: Write the failing test**
- Add a test proving that re-enabling a legacy duplicate binding fails when another workspace already owns that Feishu group.

**Step 2: Run test to verify it fails**
- Run: `DATABASE_URL=sqlite://$(pwd)/dev_assets/db.v2.sqlite cargo test -p server update_binding_rejects_enabling_group_bound_to_another_workspace -- --nocapture`
- Expected: FAIL until the update flow validates group ownership before enabling.

**Step 3: Write minimal implementation**
- Check for conflicting active bindings before applying an enabled update.

**Step 4: Run test to verify it passes**
- Re-run the same command and expect PASS.

### Task 3: Surface conflicts in the binding dialog

**Files:**
- Modify: `packages/web-core/src/pages/workspaces/FeishuBindingDialog.tsx`
- Check: `packages/web-core/src/shared/hooks/useWorkspaceFeishuBindings.ts`

**Step 1: Add a visible error state**
- Show the binding mutation error message near the top of the dialog.

**Step 2: Run a local type check**
- Run the workspace TypeScript check or the repo-wide check once the backend changes compile.

### Task 4: Update tracking and integration docs

**Files:**
- Modify: `docs/project-tracking/feishu-workspace-bot/progress.mdx`
- Modify: `docs/project-tracking/feishu-workspace-bot/decision-log.mdx`
- Modify: `docs/integrations/feishu-integration.mdx`

**Step 1: Record the rule**
- Document that each Feishu group can have only one active workspace binding at a time.

**Step 2: Record verification**
- Save the targeted test commands and the live-debugging rationale.

### Task 5: Run final verification

**Files:**
- No code changes expected

**Step 1: Run formatting**
- Run: `pnpm run format`

**Step 2: Run checks**
- Run: `pnpm run check`

**Step 3: Run lint**
- Run: `pnpm run lint`

**Step 4: Run focused backend tests**
- Run: `DATABASE_URL=sqlite://$(pwd)/dev_assets/db.v2.sqlite cargo test -p server feishu:: -- --nocapture`

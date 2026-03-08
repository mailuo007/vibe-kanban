# Feishu Reaction and Announcement Status Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add immediate Feishu receive acknowledgements via message reaction and keep group-level bot status visible through group announcements with front-end controls.

**Architecture:** Extend the Feishu client/service layer with message-reaction and group-announcement APIs, persist per-binding status-sync settings, and drive announcement updates from the existing dispatcher/runtime event flow. Expose lightweight controls in the workspace binding UI so each bound Feishu group can opt into reaction acknowledgements and announcement syncing without introducing noisy polling.

**Tech Stack:** Rust (`axum`, `sqlx`, existing Feishu service/client/runtime), React/TypeScript (`web-core`, React Query), Mintlify MDX docs.

---

### Task 1: Add failing backend tests for reaction acknowledgement

**Files:**
- Modify: `crates/server/src/feishu/dispatcher.rs`
- Check: `crates/server/src/feishu/client.rs`

**Step 1: Write the failing test**
- Add a dispatcher test that sends an inbound ordinary Feishu message and expects the client to record a reaction acknowledgement on the inbound message before any final reply.

**Step 2: Run test to verify it fails**
- Run: `DATABASE_URL=sqlite://$(pwd)/dev_assets/db.v2.sqlite cargo test -p server feishu::dispatcher::tests::adds_receive_ack_reaction_for_inbound_chat_messages -- --nocapture`
- Expected: FAIL because no reaction API or dispatcher call exists yet.

**Step 3: Write minimal implementation**
- Extend the mock Feishu client trait and dispatcher logic so inbound message handling can add a reaction when enabled.

**Step 4: Run test to verify it passes**
- Run the same command and expect PASS.

### Task 2: Add failing backend tests for group announcement status updates

**Files:**
- Modify: `crates/server/src/feishu/dispatcher.rs`
- Modify: `crates/server/src/feishu/service.rs`
- Check: `crates/db/src/models/workspace_feishu_binding.rs`

**Step 1: Write the failing test**
- Add tests that verify a binding configured for announcement sync updates group announcement content when chat starts running and when the run completes/fails.

**Step 2: Run test to verify it fails**
- Run: `DATABASE_URL=sqlite://$(pwd)/dev_assets/db.v2.sqlite cargo test -p server feishu::dispatcher::tests::updates_group_announcement_for_binding_status -- --nocapture`
- Expected: FAIL because there is no announcement sync implementation.

**Step 3: Write minimal implementation**
- Add binding flags/status text generation and client calls to fetch/update the group announcement.
- Keep refresh event-driven with a short debounce instead of fixed polling.

**Step 4: Run test to verify it passes**
- Run the same command and expect PASS.

### Task 3: Persist binding-level settings for reaction and announcement sync

**Files:**
- Create: `crates/db/migrations/20260307170000_add_feishu_binding_reaction_and_announcement_settings.sql`
- Modify: `crates/db/src/models/workspace_feishu_binding.rs`
- Modify: `crates/server/src/feishu/types.rs`
- Modify: `crates/server/src/bin/generate_types.rs`
- Modify: `shared/types.ts` (generated via command, not manually)

**Step 1: Write the failing test**
- Add/extend model tests for new default fields on workspace Feishu bindings.

**Step 2: Run test to verify it fails**
- Run the narrow Rust test target for binding defaults.

**Step 3: Write minimal implementation**
- Add migration columns for reaction acknowledgement enablement, reaction emoji type, announcement sync enablement, and optional announcement detail mode.
- Update create/update/list types.

**Step 4: Run test to verify it passes**
- Run the same test and expect PASS.

### Task 4: Extend Feishu client with reaction and announcement APIs

**Files:**
- Modify: `crates/server/src/feishu/client.rs`
- Modify: `crates/server/src/feishu/types.rs`
- Modify tests in `crates/server/src/feishu/dispatcher.rs`

**Step 1: Write the failing test**
- Add unit coverage around the mock client recording reaction and announcement calls.

**Step 2: Run test to verify it fails**
- Run the targeted dispatcher tests.

**Step 3: Write minimal implementation**
- Add client trait methods and HTTP client implementations for:
  - add message reaction
  - get group announcement
  - update group announcement
- Add graceful fallback when announcement permissions are unavailable.

**Step 4: Run test to verify it passes**
- Re-run the targeted tests.

### Task 5: Wire inbound message IDs through the long-connection path

**Files:**
- Modify: `crates/server/src/feishu/long_connection.rs`
- Modify: `crates/server/src/feishu/dispatcher.rs`
- Modify related tests

**Step 1: Write the failing test**
- Add a test that proves inbound event payloads expose the original message ID needed for reaction acknowledgement.

**Step 2: Run test to verify it fails**
- Run the long-connection test target and expect FAIL.

**Step 3: Write minimal implementation**
- Parse and pass message IDs through to dispatcher inbound handlers.

**Step 4: Run test to verify it passes**
- Re-run the same target and expect PASS.

### Task 6: Add front-end controls for the new binding settings

**Files:**
- Modify: `packages/web-core/src/pages/workspaces/FeishuBindingDialog.tsx`
- Modify: `packages/web-core/src/shared/hooks/useWorkspaceFeishuBindings.ts`
- Modify: `packages/web-core/src/shared/lib/api.ts`
- Modify locale files in `packages/web-core/src/i18n/locales/*/settings.json`

**Step 1: Write the failing test/check**
- Use TypeScript compile failures as the red signal by referencing the new fields in UI code before the API types support them.

**Step 2: Run test/check to verify it fails**
- Run: `pnpm --filter @vibe/web-core run check`
- Expected: FAIL until all new types are wired.

**Step 3: Write minimal implementation**
- Add UI toggles for:
  - receive acknowledgement reaction
  - group announcement sync
  - announcement detail mode (simple/detailed if kept)
- Do not expose raw refresh seconds.

**Step 4: Run test/check to verify it passes**
- Re-run the TypeScript check and expect PASS.

### Task 7: Update project tracking and integration docs

**Files:**
- Modify: `docs/project-tracking/feishu-workspace-bot/progress.mdx`
- Modify: `docs/project-tracking/feishu-workspace-bot/decision-log.mdx`
- Modify: `docs/project-tracking/feishu-workspace-bot/README.mdx`
- Modify: `docs/integrations/feishu-integration.mdx`

**Step 1: Document delivered behaviour**
- Explain reaction acknowledgement, event-driven announcement sync, the permission caveat, and why raw refresh intervals are not user-configurable.

**Step 2: Add validation notes**
- Record exact commands used and any permissions still required in Feishu.

### Task 8: Run final verification

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

**Step 5: Summarise remaining live validation**
- Call out any Feishu-side permissions still needed for reaction or announcement APIs.

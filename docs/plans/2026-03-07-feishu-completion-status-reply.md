# Feishu Completion Status and Reply Reliability Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Ensure Feishu-triggered workspace conversations always produce one final reply and return the group announcement from `Running` to `Idle` when the underlying execution completes.

**Architecture:** Trace the existing event bridge from execution-process persistence into the Feishu dispatcher, reproduce the missing completion callback with focused tests, and patch the binding/session lookup or patch routing so completion events reliably fan out to both reply delivery and announcement sync. Keep the fix event-driven and scoped to Feishu-linked sessions so ordinary workspace behaviour does not change.

**Tech Stack:** Rust (`axum`, `sqlx`, async task/event stream code in `server` and `services`), SQLite dev DB, Mintlify/MDX docs.

---

### Task 1: Map the live completion-event path

**Files:**
- Check: `crates/server/src/feishu/dispatcher.rs`
- Check: `crates/server/src/feishu/chat_runner.rs`
- Check: `crates/server/src/session_follow_up.rs`
- Check: `crates/services/src/services/events.rs`
- Check: `crates/services/src/services/events/patches.rs`

**Step 1: Trace the real runtime flow**
- Identify which persisted records change when a Feishu ordinary message starts a follow-up run and which event stream items the dispatcher listens to for final replies and announcement refresh.

**Step 2: Confirm the likely missing edge**
- Compare the session/process identifiers created in the Feishu follow-up path with the identifiers used in dispatcher binding lookup and completion routing.

### Task 2: Add a failing regression test for missing completion fan-out

**Files:**
- Modify: `crates/server/src/feishu/dispatcher.rs`
- Check: `crates/services/src/services/events/types.rs`

**Step 1: Write the failing test**
- Add a dispatcher-level regression test that simulates the same completion patch shape produced by the runtime for a Feishu-created follow-up session and asserts the dispatcher sends exactly one final reply and updates the announcement back to `Idle`.

**Step 2: Run test to verify it fails**
- Run: `DATABASE_URL=sqlite://$(pwd)/dev_assets/db.v2.sqlite cargo test -p server feishu::dispatcher::tests::handles_feishu_follow_up_completion_event_end_to_end -- --nocapture`
- Expected: FAIL because the current completion event is not fully routed to the Feishu binding.

### Task 3: Implement the minimal completion routing fix

**Files:**
- Modify: `crates/server/src/feishu/dispatcher.rs`
- Modify: `crates/services/src/services/events.rs` (only if event payload mapping is the actual gap)
- Modify related support types only if required

**Step 1: Patch the root cause**
- Fix the binding/session/process lookup or the event-to-dispatcher routing so Feishu-linked follow-up completions reliably trigger:
  - one final reply
  - one status transition back to `Idle`

**Step 2: Keep duplicate protection intact**
- Preserve the existing `message_id` dedupe and non-user sender filtering so this fix does not reintroduce duplicate replies.

### Task 4: Verify the regression is green

**Files:**
- No new files expected

**Step 1: Re-run the focused test**
- Run the targeted dispatcher test and confirm PASS.

**Step 2: Re-run nearby Feishu tests**
- Run a focused `feishu::` test subset to ensure announcement sync and duplicate guards still pass.

### Task 5: Update project tracking and integration docs

**Files:**
- Modify: `docs/project-tracking/feishu-workspace-bot/progress.mdx`
- Modify: `docs/project-tracking/feishu-workspace-bot/decision-log.mdx`
- Modify: `docs/integrations/feishu-integration.mdx`

**Step 1: Record the root cause**
- Document why completion could be persisted successfully while Feishu still stayed on `Running`.

**Step 2: Record the fix and validation**
- Document the exact regression test and verification commands used for the repair.

### Task 6: Run final verification

**Files:**
- No code changes expected

**Step 1: Run formatting**
- Run: `pnpm run format`

**Step 2: Run workspace checks**
- Run: `pnpm run check`

**Step 3: Run lint**
- Run: `pnpm run lint`

**Step 4: Run focused backend verification**
- Run: `DATABASE_URL=sqlite://$(pwd)/dev_assets/db.v2.sqlite cargo test -p server feishu:: -- --nocapture`

**Step 5: Call out remaining live validation**
- Note any Feishu-side behaviour that still needs manual confirmation in the real group chat.

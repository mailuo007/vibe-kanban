# Feishu Embedded Workspace Bot Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Build an embedded Feishu subsystem in `crates/server` that manages multiple Feishu application bots, binds workspace targets, sends workspace notifications, and supports safe commands and card interactions.

**Architecture:** The implementation adds a dedicated Feishu subsystem to the local server, with SQLite-backed bot and binding metadata, a secret storage abstraction, one runtime per enabled bot, workspace-scoped binding APIs, and frontend entry points in Settings and `workspaceActions`. The first release focuses on safe notifications and interactions, with strict runtime isolation and conservative permission defaults.

**Tech Stack:** Rust, Axum, SQLx/SQLite, React, TypeScript, TanStack Query, generated shared types, Feishu long connection runtime

---

### Task 1: Add Feishu database schema and model types

**Files:**
- Create: `crates/db/migrations/20260307100000_add_feishu_bot_tables.sql`
- Create: `crates/db/src/models/feishu_bot.rs`
- Create: `crates/db/src/models/feishu_bot_target.rs`
- Create: `crates/db/src/models/workspace_feishu_binding.rs`
- Create: `crates/db/src/models/feishu_conversation.rs`
- Create: `crates/db/src/models/feishu_delivery_log.rs`
- Modify: `crates/db/src/models/mod.rs`
- Modify: `crates/server/src/bin/generate_types.rs`
- Test: inline Rust unit tests in the new model files

**Step 1: Write the failing model tests**

Add unit tests that assert:

- duplicate bot names are rejected
- duplicate `(workspace_id, target_id)` bindings are rejected
- deleting a bot with targets or bindings is blocked unless explicitly handled

**Step 2: Run the model-focused tests to verify failure**

Run: `cargo test --package db feishu -- --nocapture`

Expected: FAIL because the migration and models do not exist yet.

**Step 3: Add the migration and model implementations**

Create tables and Rust models for:

```rust
pub struct FeishuBot { /* id, name, app_id, enabled, health */ }
pub struct FeishuBotTarget { /* bot_id, target_type, chat ids */ }
pub struct WorkspaceFeishuBinding { /* workspace_id, bot_id, target_id, flags */ }
pub struct FeishuConversation { /* bot_id, target_id, workspace_id, context */ }
pub struct FeishuDeliveryLog { /* workspace_id, status, message_id, error */ }
```

Update `crates/db/src/models/mod.rs` and add any generated type declarations to `crates/server/src/bin/generate_types.rs`.

**Step 4: Run the model tests again**

Run: `cargo test --package db feishu -- --nocapture`

Expected: PASS for the new model tests.

**Step 5: Commit**

```bash
git add crates/db/migrations/20260307100000_add_feishu_bot_tables.sql \
  crates/db/src/models/feishu_bot.rs \
  crates/db/src/models/feishu_bot_target.rs \
  crates/db/src/models/workspace_feishu_binding.rs \
  crates/db/src/models/feishu_conversation.rs \
  crates/db/src/models/feishu_delivery_log.rs \
  crates/db/src/models/mod.rs \
  crates/server/src/bin/generate_types.rs
git commit -m "feat: add feishu bot persistence schema"
```

### Task 2: Add secret storage and Feishu service boundaries

**Files:**
- Create: `crates/server/src/feishu/mod.rs`
- Create: `crates/server/src/feishu/secret_store.rs`
- Create: `crates/server/src/feishu/types.rs`
- Create: `crates/server/src/feishu/service.rs`
- Modify: `crates/server/src/lib.rs`
- Modify: `crates/deployment/src/lib.rs`
- Modify: `crates/local-deployment/src/lib.rs`
- Test: `crates/server/src/feishu/service.rs`

**Step 1: Write the failing service tests**

Add tests that assert:

- secrets are resolved by reference rather than stored inline
- bot validation rejects incomplete credentials
- bindings default to safe permissions

**Step 2: Run the new service tests**

Run: `cargo test --package server feishu::service -- --nocapture`

Expected: FAIL because the Feishu module and deployment accessors do not exist.

**Step 3: Implement the secret store and service skeleton**

Add a server-local abstraction like:

```rust
pub trait FeishuSecretStore {
    async fn put(&self, value: SecretString) -> anyhow::Result<String>;
    async fn get(&self, reference: &str) -> anyhow::Result<SecretString>;
}

pub struct FeishuService {
    /* db, secret store, config, runtime manager handle */
}
```

Expose the service through deployment so routes can access it cleanly.

**Step 4: Re-run the service tests**

Run: `cargo test --package server feishu::service -- --nocapture`

Expected: PASS for secret resolution and default permission rules.

**Step 5: Commit**

```bash
git add crates/server/src/feishu/mod.rs \
  crates/server/src/feishu/secret_store.rs \
  crates/server/src/feishu/types.rs \
  crates/server/src/feishu/service.rs \
  crates/server/src/lib.rs \
  crates/deployment/src/lib.rs \
  crates/local-deployment/src/lib.rs
git commit -m "feat: add feishu service and secret storage"
```

### Task 3: Build runtime manager and per-bot runtime isolation

**Files:**
- Create: `crates/server/src/feishu/runtime_manager.rs`
- Create: `crates/server/src/feishu/runtime.rs`
- Create: `crates/server/src/feishu/client.rs`
- Modify: `crates/local-deployment/src/lib.rs`
- Test: `crates/server/src/feishu/runtime_manager.rs`

**Step 1: Write the failing runtime tests**

Add tests that verify:

- one enabled bot starts one runtime
- disabling one bot stops only that runtime
- updating credentials rebuilds only the changed runtime
- failed connection attempts move the bot into `reconnecting` or `degraded`

**Step 2: Run the runtime tests**

Run: `cargo test --package server feishu::runtime_manager -- --nocapture`

Expected: FAIL because the manager and runtime do not exist.

**Step 3: Implement the runtime manager**

Use one task per bot and a manager API similar to:

```rust
pub struct FeishuRuntimeManager { /* map bot_id -> runtime handle */ }

impl FeishuRuntimeManager {
    pub async fn restore_enabled_bots(&self) -> anyhow::Result<()>;
    pub async fn upsert_runtime(&self, bot_id: Uuid) -> anyhow::Result<()>;
    pub async fn stop_runtime(&self, bot_id: Uuid) -> anyhow::Result<()>;
}
```

Inject the manager into `LocalDeployment` during startup and restore enabled bots after DB and secret store initialisation.

**Step 4: Run the runtime tests again**

Run: `cargo test --package server feishu::runtime_manager -- --nocapture`

Expected: PASS for startup, isolation, and health-state behaviour.

**Step 5: Commit**

```bash
git add crates/server/src/feishu/runtime_manager.rs \
  crates/server/src/feishu/runtime.rs \
  crates/server/src/feishu/client.rs \
  crates/local-deployment/src/lib.rs
git commit -m "feat: add feishu runtime manager"
```

### Task 4: Add Feishu management and workspace binding routes

**Files:**
- Create: `crates/server/src/routes/feishu.rs`
- Create: `crates/server/src/routes/task_attempts/feishu.rs`
- Modify: `crates/server/src/routes/mod.rs`
- Modify: `crates/server/src/routes/task_attempts.rs`
- Modify: `crates/server/src/bin/generate_types.rs`
- Test: inline route tests in `crates/server/src/routes/feishu.rs`

**Step 1: Write the failing route tests**

Cover:

- list bots
- create bot
- validate bot
- list bot targets
- list workspace bindings
- create and delete workspace binding

**Step 2: Run the route tests**

Run: `cargo test --package server routes::feishu -- --nocapture`

Expected: FAIL because the route modules are missing.

**Step 3: Implement the routes**

Add global routes:

```rust
Router::new()
  .route("/feishu/bots", get(list_bots).post(create_bot))
  .route("/feishu/bots/{bot_id}", get(get_bot).patch(update_bot).delete(delete_bot))
  .route("/feishu/bots/{bot_id}/validate", post(validate_bot))
  .route("/feishu/bots/{bot_id}/targets", get(list_targets))
  .route("/feishu/bots/{bot_id}/targets/discover", post(discover_targets))
```

Add workspace routes under `task_attempts` for bindings and bindable targets.

**Step 4: Run the route tests again**

Run: `cargo test --package server routes::feishu -- --nocapture`

Expected: PASS for route creation and happy-path service wiring.

**Step 5: Commit**

```bash
git add crates/server/src/routes/feishu.rs \
  crates/server/src/routes/task_attempts/feishu.rs \
  crates/server/src/routes/mod.rs \
  crates/server/src/routes/task_attempts.rs \
  crates/server/src/bin/generate_types.rs
git commit -m "feat: add feishu management and binding routes"
```

### Task 5: Bridge outbound notifications and inbound Feishu interactions

**Files:**
- Create: `crates/server/src/feishu/dispatcher.rs`
- Modify: `crates/server/src/routes/events.rs`
- Modify: `crates/server/src/feishu/service.rs`
- Modify: `crates/server/src/feishu/runtime.rs`
- Test: `crates/server/src/feishu/dispatcher.rs`

**Step 1: Write the failing dispatcher tests**

Add tests for:

- one workspace event fan-outs to all enabled bindings
- duplicate inbound events are ignored
- safe commands route to the correct workspace
- destructive commands are rejected when the binding disallows them

**Step 2: Run the dispatcher tests**

Run: `cargo test --package server feishu::dispatcher -- --nocapture`

Expected: FAIL because the dispatcher is not implemented.

**Step 3: Implement event routing**

Build an inbound and outbound dispatcher that:

- translates server workspace events into Feishu deliveries
- resolves inbound chat and card context through `feishu_conversations`
- supports `/vk help`, `/vk status`, `/vk open`, `/vk stop`, `/vk archive`, `/vk new`
- requires templates for `/vk new`

**Step 4: Run the dispatcher tests again**

Run: `cargo test --package server feishu::dispatcher -- --nocapture`

Expected: PASS for fan-out, idempotency, and permission gating.

**Step 5: Commit**

```bash
git add crates/server/src/feishu/dispatcher.rs \
  crates/server/src/routes/events.rs \
  crates/server/src/feishu/service.rs \
  crates/server/src/feishu/runtime.rs
git commit -m "feat: wire feishu notifications and commands"
```

### Task 6: Expose frontend API methods and hooks

**Files:**
- Modify: `packages/web-core/src/shared/lib/api.ts`
- Create: `packages/web-core/src/shared/hooks/useFeishuBots.ts`
- Create: `packages/web-core/src/shared/hooks/useWorkspaceFeishuBindings.ts`
- Modify: `shared/types.ts` (generated via `pnpm run generate-types`, do not edit by hand)
- Test: type-level validation via existing workspace checks

**Step 1: Write the failing frontend type usage**

Create hooks that import the future API methods so TypeScript fails until the API surface exists.

**Step 2: Run the typecheck**

Run: `pnpm run check`

Expected: FAIL because the Feishu API methods and types are missing.

**Step 3: Implement API client methods and hooks**

Add:

- `feishuApi.listBots()`
- `feishuApi.createBot()`
- `feishuApi.validateBot()`
- `feishuApi.listTargets()`
- `attemptsApi.listFeishuBindings(attemptId)`
- `attemptsApi.createFeishuBinding(attemptId, payload)`
- `attemptsApi.deleteFeishuBinding(attemptId, bindingId)`

Wrap them in React Query hooks for settings and workspace UI.

**Step 4: Run the typecheck again**

Run: `pnpm run generate-types && pnpm run check`

Expected: PASS with the new types and API methods available.

**Step 5: Commit**

```bash
git add packages/web-core/src/shared/lib/api.ts \
  packages/web-core/src/shared/hooks/useFeishuBots.ts \
  packages/web-core/src/shared/hooks/useWorkspaceFeishuBindings.ts \
  shared/types.ts
git commit -m "feat: add feishu frontend api hooks"
```

### Task 7: Add Feishu settings UI

**Files:**
- Create: `packages/web-core/src/shared/dialogs/settings/settings/FeishuSettingsSection.tsx`
- Modify: `packages/web-core/src/shared/dialogs/settings/settings/SettingsSection.tsx`
- Modify: `packages/web-core/src/shared/dialogs/settings/SettingsDialog.tsx`
- Modify: `packages/web-core/src/i18n/locales/en/settings.json`
- Modify: `packages/web-core/src/i18n/locales/es/settings.json`
- Modify: `packages/web-core/src/i18n/locales/fr/settings.json`
- Modify: `packages/web-core/src/i18n/locales/ja/settings.json`
- Modify: `packages/web-core/src/i18n/locales/ko/settings.json`
- Modify: `packages/web-core/src/i18n/locales/zh-Hans/settings.json`
- Modify: `packages/web-core/src/i18n/locales/zh-Hant/settings.json`

**Step 1: Build the section shell and let the typecheck fail**

Add a new `SettingsSectionType` entry for `feishu` and reference a section component that does not exist yet.

**Step 2: Run the typecheck**

Run: `pnpm run check`

Expected: FAIL because the new section component and translation keys are missing.

**Step 3: Implement the settings section**

Build a section that supports:

- bot list
- add and edit form
- validate action
- enable or disable toggle
- target refresh
- health and last-error display

Keep styling consistent with `SettingsComponents` and other settings sections.

**Step 4: Run the typecheck and lint**

Run: `pnpm run check && pnpm run lint`

Expected: PASS for settings UI integration.

**Step 5: Commit**

```bash
git add packages/web-core/src/shared/dialogs/settings/settings/FeishuSettingsSection.tsx \
  packages/web-core/src/shared/dialogs/settings/settings/SettingsSection.tsx \
  packages/web-core/src/shared/dialogs/settings/SettingsDialog.tsx \
  packages/web-core/src/i18n/locales/en/settings.json \
  packages/web-core/src/i18n/locales/es/settings.json \
  packages/web-core/src/i18n/locales/fr/settings.json \
  packages/web-core/src/i18n/locales/ja/settings.json \
  packages/web-core/src/i18n/locales/ko/settings.json \
  packages/web-core/src/i18n/locales/zh-Hans/settings.json \
  packages/web-core/src/i18n/locales/zh-Hant/settings.json
git commit -m "feat: add feishu settings section"
```

### Task 8: Add workspace actions for binding and management

**Files:**
- Modify: `packages/web-core/src/shared/actions/index.ts`
- Modify: `packages/web-core/src/shared/command-bar/actions/pages.ts`
- Create: `packages/web-core/src/pages/workspaces/FeishuBindingDialog.tsx`
- Modify: `packages/web-core/src/pages/workspaces/WorkspacesSidebarContainer.tsx`
- Modify: `packages/web-core/src/i18n/locales/en/common.json`
- Modify: `packages/web-core/src/i18n/locales/es/common.json`
- Modify: `packages/web-core/src/i18n/locales/fr/common.json`
- Modify: `packages/web-core/src/i18n/locales/ja/common.json`
- Modify: `packages/web-core/src/i18n/locales/ko/common.json`
- Modify: `packages/web-core/src/i18n/locales/zh-Hans/common.json`
- Modify: `packages/web-core/src/i18n/locales/zh-Hant/common.json`

**Step 1: Add the command-bar actions and let the UI fail typecheck**

Reference new actions:

- `BindFeishuBot`
- `ManageFeishuBindings`

**Step 2: Run the typecheck**

Run: `pnpm run check`

Expected: FAIL because the new actions and dialog are missing.

**Step 3: Implement the binding UI**

Create a binding dialog or flow that:

- loads bindable targets for the current workspace
- performs one-click binding when there is one unambiguous candidate
- shows current bindings
- toggles allowed capabilities
- sends test messages
- unbinds a target

**Step 4: Run the typecheck and manual smoke check**

Run: `pnpm run check`

Expected: PASS. Then manually verify the workspace actions menu opens the Feishu binding flow.

**Step 5: Commit**

```bash
git add packages/web-core/src/shared/actions/index.ts \
  packages/web-core/src/shared/command-bar/actions/pages.ts \
  packages/web-core/src/pages/workspaces/FeishuBindingDialog.tsx \
  packages/web-core/src/pages/workspaces/WorkspacesSidebarContainer.tsx \
  packages/web-core/src/i18n/locales/en/common.json \
  packages/web-core/src/i18n/locales/es/common.json \
  packages/web-core/src/i18n/locales/fr/common.json \
  packages/web-core/src/i18n/locales/ja/common.json \
  packages/web-core/src/i18n/locales/ko/common.json \
  packages/web-core/src/i18n/locales/zh-Hans/common.json \
  packages/web-core/src/i18n/locales/zh-Hant/common.json
git commit -m "feat: add workspace feishu binding actions"
```

### Task 9: Verify, document, and harden the feature

**Files:**
- Modify: `docs/project-tracking/feishu-workspace-bot/progress.mdx`
- Modify: `docs/project-tracking/feishu-workspace-bot/decision-log.mdx`
- Modify: `docs/project-tracking/feishu-workspace-bot/design-draft.mdx`
- Create: `docs/integrations/feishu-integration.mdx`

**Step 1: Add final verification checklist**

Document the exact local manual verification flow in the progress tracker and final integration doc.

**Step 2: Run verification commands**

Run:

```bash
cargo test --workspace
pnpm run generate-types
pnpm run check
pnpm run lint
pnpm run format
```

Expected: all commands pass.

**Step 3: Perform the local end-to-end validation**

Verify:

- register bot
- validate bot
- bind workspace
- send test message
- receive notification
- run safe command
- click safe card action
- restart server and confirm runtime restoration

**Step 4: Update docs**

Write user-facing setup guidance in:

- `docs/integrations/feishu-integration.mdx`

Update internal tracking docs with the implementation result and any changes from the approved design.

**Step 5: Commit**

```bash
git add docs/project-tracking/feishu-workspace-bot/progress.mdx \
  docs/project-tracking/feishu-workspace-bot/decision-log.mdx \
  docs/project-tracking/feishu-workspace-bot/design-draft.mdx \
  docs/integrations/feishu-integration.mdx
git commit -m "docs: add feishu integration documentation"
```

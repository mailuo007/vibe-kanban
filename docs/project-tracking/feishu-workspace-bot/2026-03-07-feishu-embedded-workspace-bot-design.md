# Feishu Embedded Workspace Bot Design

## Summary

This design adds an embedded Feishu subsystem to `crates/server` so Vibe Kanban can:

- manage a global pool of Feishu self-built application bots
- bind one `workspace` to one or more bot targets
- send workspace lifecycle updates to Feishu
- accept safe Feishu commands and card interactions for bound workspaces

The chosen model is a global bot pool with workspace-level bindings.

## Goals

- Embed Feishu support into the local server runtime rather than a sidecar
- Support multiple Feishu applications
- Support multiple chats or sessions under each application
- Let each workspace bind to multiple Feishu targets from `workspaceActions`
- Provide a path for one-click binding when there is exactly one unambiguous target
- Keep runtime failures isolated per bot

## Non-goals

- Do not support public callback mode in the first iteration
- Do not guess workspace creation parameters from arbitrary free text
- Do not store Feishu secrets in the general config JSON
- Do not make Feishu delivery a hard dependency for core workspace actions

## Constraints

- The target environment is local operation on the user's machine
- The system must use Feishu long connection mode for the embedded runtime
- The current workspace actions surface is the preferred entry point for binding
- The existing local server and task-attempt route model should be reused where possible

## Architecture

Treat Feishu as a dedicated subsystem inside `crates/server`.

### Server modules

Create a new `crates/server/src/feishu/` module with the following responsibilities:

- `runtime_manager`: lifecycle management for all enabled bots
- `runtime`: one isolated runtime per bot
- `client`: outbound Feishu API wrapper
- `service`: business rules for bots, targets, bindings, and validation
- `dispatcher`: inbound and outbound event routing
- `secret_store`: secret lookup and persistence abstraction
- `types`: server-local request, response, and health types if needed

### Integration boundaries

- `runtime` does not mutate workspace records directly
- `service` owns database mutations and validation
- `dispatcher` translates Feishu events into calls to existing server services or routes
- `routes` expose only local management and binding APIs

### Startup model

- The local server creates a `FeishuRuntimeManager` during deployment initialisation
- The manager loads enabled bots from the database
- One runtime task starts per bot
- Bot updates rebuild only the affected runtime

## Data model

Persist business relationships in SQLite.

### `feishu_bots`

Stores one logical Feishu application bot.

Suggested fields:

- `id`
- `name`
- `app_id`
- `app_secret_ref`
- `encrypt_key_ref`
- `verification_token_ref`
- `tenant_mode`
- `enabled`
- `last_health_status`
- `last_error`
- `created_at`
- `updated_at`

### `feishu_bot_targets`

Stores one sendable destination under a bot.

Suggested fields:

- `id`
- `bot_id`
- `target_type`
- `open_chat_id`
- `chat_id`
- `name`
- `source`
- `is_active`
- `created_at`
- `updated_at`

### `workspace_feishu_bindings`

Stores one binding between one workspace and one Feishu target.

Suggested fields:

- `id`
- `workspace_id`
- `bot_id`
- `target_id`
- `enabled`
- `notify_on_status`
- `notify_on_agent_reply`
- `notify_on_pr`
- `allow_commands`
- `allow_cards`
- `created_at`
- `updated_at`

### `feishu_conversations`

Stores inbound context to route future events back to the correct workspace.

Suggested fields:

- `id`
- `bot_id`
- `target_id`
- `workspace_id`
- `feishu_user_id`
- `last_message_at`
- `last_card_context`
- `created_at`
- `updated_at`

### `feishu_delivery_logs`

Stores outbound attempts and failures for observability.

Suggested fields:

- `id`
- `workspace_id`
- `bot_id`
- `target_id`
- `event_type`
- `payload_summary`
- `status`
- `retry_count`
- `message_id`
- `error_message`
- `sent_at`

## Secret storage

Store business metadata and relationships in SQLite, but do not store plaintext Feishu secrets in the general config file.

Use a dedicated secret storage abstraction:

- DB rows keep stable secret references
- secret values are loaded by the runtime when needed
- the first implementation may use a local restricted-permission file store
- the abstraction should allow a later upgrade to macOS Keychain without changing higher layers

## API design

Use global routes for bot management and workspace-scoped routes for bindings.

### Global management routes

Prefix: `/api/feishu/*`

- `GET /api/feishu/bots`
- `POST /api/feishu/bots`
- `GET /api/feishu/bots/{botId}`
- `PATCH /api/feishu/bots/{botId}`
- `DELETE /api/feishu/bots/{botId}`
- `POST /api/feishu/bots/{botId}/validate`
- `GET /api/feishu/bots/{botId}/targets`
- `POST /api/feishu/bots/{botId}/targets/discover`

### Workspace binding routes

Prefix: `/api/task-attempts/{id}/feishu/*`

- `GET /api/task-attempts/{id}/feishu/bindings`
- `GET /api/task-attempts/{id}/feishu/bindable-targets`
- `POST /api/task-attempts/{id}/feishu/bindings`
- `PATCH /api/task-attempts/{id}/feishu/bindings/{bindingId}`
- `DELETE /api/task-attempts/{id}/feishu/bindings/{bindingId}`
- `POST /api/task-attempts/{id}/feishu/bindings/{bindingId}/test`

## UI design

### Global settings

Add a new Feishu settings section to manage the global bot pool.

The settings flow should let you:

- add a bot
- validate credentials
- enable or disable a bot
- inspect health and last error
- view and refresh discovered targets

### Workspace actions

Add two actions to `workspaceActions`:

- `Bind Feishu Bot`
- `Manage Feishu Bindings`

`Bind Feishu Bot` should:

- immediately bind when exactly one bot and one valid target are available
- otherwise open a selection flow

`Manage Feishu Bindings` should:

- show all current bindings for the workspace
- toggle notification and interaction permissions
- send a test message
- remove a binding

## Event mapping

### Outbound notifications

The first pass should send workspace-scoped events:

- workspace created
- workspace renamed
- workspace archived
- execution started
- execution completed
- execution failed
- pull request created
- pull request status changed

### Card interactions

The first pass should support:

- view status
- open workspace
- stop execution
- archive workspace
- mute notifications

Dangerous actions should require confirmation.

### Message commands

The first pass should support:

- `/vk help`
- `/vk status`
- `/vk open`
- `/vk stop`
- `/vk archive`
- `/vk new`

`/vk new` should require a configured default template rather than guessing missing creation parameters.

## Runtime design

### Runtime manager

One `FeishuRuntimeManager` controls bot lifecycle.

Responsibilities:

- restore enabled bots at startup
- create one runtime per bot
- rebuild runtimes when credentials change
- stop runtimes when disabled
- surface health state to the UI

### Per-bot runtime

Each bot runtime owns:

- long connection startup and shutdown
- reconnect policy
- inbound event handling
- outbound message sending for that bot
- recent error state
- current health state

### Isolation and resilience

- one broken bot does not affect other bots
- one failing target does not block other targets
- one repeated inbound event does not trigger duplicate state changes
- delivery failures are logged and retried a bounded number of times

### Health states

Track:

- `starting`
- `healthy`
- `degraded`
- `reconnecting`
- `disabled`
- `failed`

## Validation

### Backend

- migration tests for schema and constraints
- service tests for bot registration, target binding, and validation rules
- runtime manager tests for startup, shutdown, isolation, and hot reload
- dispatcher tests for idempotency and correct routing
- route tests for global and workspace-scoped endpoints

### Frontend

- typecheck and lint validation for new settings and workspace actions UI
- manual verification for bind, unbind, test send, and health display

### End-to-end

Run a local manual flow:

1. register and validate a bot
2. bind a workspace to one or more targets
3. send a test message
4. trigger a workspace event and observe notification delivery
5. run a safe Feishu command such as `/vk status`
6. click a safe card action
7. restart the server and confirm runtime restoration

## Risks and mitigations

- **Secret leakage risk**: keep secrets out of general config and resolve by reference
- **Runtime flapping risk**: use backoff and jitter on reconnects
- **Duplicate event risk**: make inbound handling idempotent
- **Overly broad permissions risk**: default destructive capabilities to disabled
- **UI complexity risk**: keep global bot management in Settings and workspace binding in workspace actions

## Delivery milestones

### Milestone 1

- schema
- secret storage abstraction
- runtime manager
- global bot settings
- health display

### Milestone 2

- workspace binding APIs
- workspace actions UI
- test send
- outbound notifications

### Milestone 3

- safe commands
- safe card interactions
- default creation templates
- improved delivery observability

## Related working files

- `docs/project-tracking/feishu-workspace-bot/design-draft.mdx`
- `docs/project-tracking/feishu-workspace-bot/progress.mdx`
- `docs/project-tracking/feishu-workspace-bot/decision-log.mdx`

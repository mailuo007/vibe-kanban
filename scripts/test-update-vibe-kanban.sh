#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

FIXTURE_REPO="$TMP_DIR/repo"

mkdir -p \
  "$FIXTURE_REPO/crates/server/src/bin" \
  "$FIXTURE_REPO/crates/server/src/routes/sessions" \
  "$FIXTURE_REPO/crates/server/src/routes/workspaces" \
  "$FIXTURE_REPO/packages/web-core/src/shared/lib"

cat <<'EOF' > "$FIXTURE_REPO/crates/server/src/bin/generate_types.rs"
        db::models::requests::UpdateWorkspace::decl(),
<<<<<<< HEAD
        server::routes::task_attempts::workspace_summary::WorkspaceSummaryRequest::decl(),
        server::routes::task_attempts::workspace_summary::WorkspaceSummary::decl(),
        server::routes::task_attempts::workspace_summary::WorkspaceSummaryResponse::decl(),
        server::routes::task_attempts::workspace_summary::DiffStats::decl(),
        server::routes::feishu::CreateFeishuBotRequest::decl(),
        server::routes::feishu::UpdateFeishuBotRequest::decl(),
        server::routes::feishu::ValidateFeishuBotResponse::decl(),
        server::routes::feishu::FeishuInboundCommandRequest::decl(),
        server::routes::feishu::FeishuInboundCardActionRequest::decl(),
        server::routes::feishu::FeishuInboundActionResponse::decl(),
        server::routes::task_attempts::feishu::CreateWorkspaceFeishuBindingRequest::decl(),
        server::routes::task_attempts::feishu::UpdateWorkspaceFeishuBindingRequest::decl(),
        server::routes::task_attempts::feishu::WorkspaceFeishuBindTarget::decl(),
        server::routes::task_attempts::feishu::SendWorkspaceFeishuTestMessageRequest::decl(),
=======
        server::routes::workspaces::workspace_summary::WorkspaceSummaryRequest::decl(),
        server::routes::workspaces::workspace_summary::WorkspaceSummary::decl(),
        server::routes::workspaces::workspace_summary::WorkspaceSummaryResponse::decl(),
        server::routes::workspaces::workspace_summary::DiffStats::decl(),
>>>>>>> upstream/main
EOF

cat <<'EOF' > "$FIXTURE_REPO/crates/server/src/routes/sessions/mod.rs"
use crate::{
<<<<<<< HEAD
    DeploymentImpl,
    error::ApiError,
    middleware::load_session_middleware,
    routes::workspaces::RunScriptError,
    session_follow_up::{SessionFollowUpRequest, start_session_follow_up},
=======
    DeploymentImpl, error::ApiError, middleware::load_session_middleware,
    routes::workspaces::execution::RunScriptError,
>>>>>>> upstream/main
};
EOF

cat <<'EOF' > "$FIXTURE_REPO/crates/server/src/routes/workspaces.rs"
pub mod execution;

use axum::{
    Router,
    routing::{get, post},
};

pub fn router() -> Router<()> {
    Router::new()
        .route("/streams/ws", get(|| async {}))
        .route("/summaries", post(|| async {}))
}
EOF

cat <<'EOF' > "$FIXTURE_REPO/crates/server/src/routes/workspaces/mod.rs"
pub mod execution;

use axum::{
    Router,
    middleware::from_fn_with_state,
    routing::{get, post},
};

use crate::{DeploymentImpl, middleware::load_workspace_middleware};

pub fn router() -> Router<()> {
    let workspace_id_router = Router::new()
        .route("/", get(|| async {}))
        .layer(from_fn_with_state((), load_workspace_middleware));

    Router::new()
        .route("/streams/ws", get(|| async {}))
        .route("/summaries", post(|| async {}))
        .nest("/{id}", workspace_id_router)
}
EOF

cat <<'EOF' > "$FIXTURE_REPO/crates/server/src/routes/workspaces/feishu.rs"
pub fn routes() -> &'static str {
    "/task-attempts/{id}/feishu /task-attempts/{workspace_id}/feishu/bindings/{binding_id}"
}
EOF

cat <<'EOF' > "$FIXTURE_REPO/packages/web-core/src/shared/lib/api.ts"
export const oldRoute = '/api/task-attempts/123/feishu/bindings';
EOF

python3 "$ROOT_DIR/scripts/resolve_update_vibe_kanban_conflicts.py" "$FIXTURE_REPO"

grep -F 'server::routes::workspaces::workspace_summary::WorkspaceSummaryRequest::decl(),' \
  "$FIXTURE_REPO/crates/server/src/bin/generate_types.rs" >/dev/null
grep -F 'server::routes::workspaces::feishu::CreateWorkspaceFeishuBindingRequest::decl(),' \
  "$FIXTURE_REPO/crates/server/src/bin/generate_types.rs" >/dev/null
grep -F 'routes::workspaces::execution::RunScriptError,' \
  "$FIXTURE_REPO/crates/server/src/routes/sessions/mod.rs" >/dev/null
grep -F 'session_follow_up::{SessionFollowUpRequest, start_session_follow_up},' \
  "$FIXTURE_REPO/crates/server/src/routes/sessions/mod.rs" >/dev/null
grep -F 'pub mod feishu;' \
  "$FIXTURE_REPO/crates/server/src/routes/workspaces/mod.rs" >/dev/null
grep -F '.nest("/{id}/feishu", feishu::router())' \
  "$FIXTURE_REPO/crates/server/src/routes/workspaces/mod.rs" >/dev/null
grep -F '/workspaces/{id}/feishu' \
  "$FIXTURE_REPO/crates/server/src/routes/workspaces/feishu.rs" >/dev/null
grep -F '/api/workspaces/123/feishu/bindings' \
  "$FIXTURE_REPO/packages/web-core/src/shared/lib/api.ts" >/dev/null

if [ -e "$FIXTURE_REPO/crates/server/src/routes/workspaces.rs" ]; then
  echo "expected legacy workspaces.rs to be removed" >&2
  exit 1
fi

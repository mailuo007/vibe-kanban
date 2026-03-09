#!/usr/bin/env python3
from __future__ import annotations

import re
import sys
from pathlib import Path


def read(path: Path) -> str:
    return path.read_text()


def write(path: Path, text: str) -> None:
    path.write_text(text)


def replace_exact(path: Path, old: str, new: str) -> None:
    text = read(path)
    if old not in text:
        return
    write(path, text.replace(old, new))


def replace_regex(path: Path, pattern: str, replacement: str) -> None:
    text = read(path)
    updated, count = re.subn(pattern, replacement, text, flags=re.DOTALL)
    if count:
        write(path, updated)


def update_generate_types(repo: Path) -> None:
    path = repo / "crates/server/src/bin/generate_types.rs"
    if not path.exists():
        return

    replace_exact(
        path,
        """        db::models::requests::UpdateWorkspace::decl(),
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
""",
        """        db::models::requests::UpdateWorkspace::decl(),
        server::routes::workspaces::workspace_summary::WorkspaceSummaryRequest::decl(),
        server::routes::workspaces::workspace_summary::WorkspaceSummary::decl(),
        server::routes::workspaces::workspace_summary::WorkspaceSummaryResponse::decl(),
        server::routes::workspaces::workspace_summary::DiffStats::decl(),
        server::routes::feishu::CreateFeishuBotRequest::decl(),
        server::routes::feishu::UpdateFeishuBotRequest::decl(),
        server::routes::feishu::ValidateFeishuBotResponse::decl(),
        server::routes::feishu::FeishuInboundCommandRequest::decl(),
        server::routes::feishu::FeishuInboundCardActionRequest::decl(),
        server::routes::feishu::FeishuInboundActionResponse::decl(),
        server::routes::workspaces::feishu::CreateWorkspaceFeishuBindingRequest::decl(),
        server::routes::workspaces::feishu::UpdateWorkspaceFeishuBindingRequest::decl(),
        server::routes::workspaces::feishu::WorkspaceFeishuBindTarget::decl(),
        server::routes::workspaces::feishu::SendWorkspaceFeishuTestMessageRequest::decl(),
""",
    )


def update_sessions_mod(repo: Path) -> None:
    path = repo / "crates/server/src/routes/sessions/mod.rs"
    if not path.exists():
        return

    canonical = """use crate::{
    DeploymentImpl,
    error::ApiError,
    middleware::load_session_middleware,
    routes::workspaces::execution::RunScriptError,
    session_follow_up::{SessionFollowUpRequest, start_session_follow_up},
};
"""

    replace_regex(
        path,
        r"use crate::\{\n.*?middleware::load_session_middleware,.*?\n\};\n",
        canonical,
    )


def update_workspaces_mod(repo: Path) -> None:
    path = repo / "crates/server/src/routes/workspaces/mod.rs"
    if not path.exists():
        return

    text = read(path)

    if "pub mod feishu;" not in text:
        anchor = "pub mod execution;\n"
        if anchor in text:
            text = text.replace(anchor, f"{anchor}pub mod feishu;\n", 1)

    route_line = '        .nest("/{id}/feishu", feishu::router())\n'
    route_anchor = '        .nest("/{id}", workspace_id_router)\n'
    if route_line not in text and route_anchor in text:
        text = text.replace(route_anchor, f"{route_line}{route_anchor}", 1)

    write(path, text)


def update_feishu_paths(repo: Path) -> None:
    path = repo / "crates/server/src/routes/workspaces/feishu.rs"
    if not path.exists():
        return

    text = read(path)
    text = text.replace("/task-attempts/{id}/feishu", "/workspaces/{id}/feishu")
    text = text.replace(
        "/task-attempts/{workspace_id}/feishu/bindings/{binding_id}",
        "/workspaces/{workspace_id}/feishu/bindings/{binding_id}",
    )
    write(path, text)


def update_api_paths(repo: Path) -> None:
    path = repo / "packages/web-core/src/shared/lib/api.ts"
    if not path.exists():
        return

    write(path, read(path).replace("/api/task-attempts/", "/api/workspaces/"))


def remove_legacy_workspaces_file(repo: Path) -> None:
    legacy = repo / "crates/server/src/routes/workspaces.rs"
    modular = repo / "crates/server/src/routes/workspaces/mod.rs"

    if legacy.exists() and modular.exists():
        legacy.unlink()


def main() -> int:
    if len(sys.argv) != 2:
        print("usage: resolve_update_vibe_kanban_conflicts.py <repo>", file=sys.stderr)
        return 1

    repo = Path(sys.argv[1]).resolve()
    update_generate_types(repo)
    update_sessions_mod(repo)
    update_workspaces_mod(repo)
    update_feishu_paths(repo)
    update_api_paths(repo)
    remove_legacy_workspaces_file(repo)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

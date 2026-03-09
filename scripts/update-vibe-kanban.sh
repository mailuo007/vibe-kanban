#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

REPO="${REPO:-/Volumes/ydyp/vibe-kanban}"
BRANCH="${BRANCH:-vibe-kanban01}"
UPSTREAM_REMOTE="${UPSTREAM_REMOTE:-upstream}"
UPSTREAM_BRANCH="${UPSTREAM_BRANCH:-main}"
PATCH_FILE="${PATCH_FILE:-/Users/baozi/feishu-binding-fix.patch}"
SKIP_PUSH="${SKIP_PUSH:-0}"

say() {
  printf '[vibe-kanban] %s\n' "$1"
}

fail() {
  printf '[vibe-kanban] %s\n' "$1" >&2
  exit 1
}

in_merge() {
  git rev-parse -q --verify MERGE_HEAD >/dev/null 2>&1
}

ensure_repo() {
  [ -d "$REPO/.git" ] || fail "找不到仓库：$REPO"
}

ensure_clean_tracked() {
  if in_merge; then
    return 0
  fi

  git diff --quiet || fail "有未提交修改，先 commit 或 stash 再更新。"
  git diff --cached --quiet || fail "有已暂存未提交修改，先 commit 再更新。"
}

resolve_known_merge_conflicts() {
  say "检测到冲突，开始自动修复已知冲突"

  python3 "$SCRIPT_DIR/resolve_update_vibe_kanban_conflicts.py" "$REPO"

  git add \
    crates/server/src/bin/generate_types.rs \
    crates/server/src/routes/sessions/mod.rs \
    crates/server/src/routes/workspaces/mod.rs \
    crates/server/src/routes/workspaces/feishu.rs \
    packages/web-core/src/shared/lib/api.ts

  git rm -f --ignore-unmatch crates/server/src/routes/workspaces.rs >/dev/null 2>&1 || true
  git rm -f --ignore-unmatch crates/server/src/routes/task_attempts.rs >/dev/null 2>&1 || true

  if [ -n "$(git ls-files -u)" ]; then
    fail "还有未解决冲突，请手动处理后再执行一次。"
  fi

  git commit -m "Merge ${UPSTREAM_REMOTE}/${UPSTREAM_BRANCH} into ${BRANCH}"
  say "冲突已自动解决并提交"
}

apply_local_patch() {
  if [ ! -f "$PATCH_FILE" ]; then
    say "未找到本地补丁，跳过：$PATCH_FILE"
    return 0
  fi

  if git apply --reverse --check "$PATCH_FILE" >/dev/null 2>&1; then
    say "本地飞书修复已经在当前分支，跳过补丁"
    return 0
  fi

  if git apply --check "$PATCH_FILE" >/dev/null 2>&1; then
    say "应用本地飞书修复补丁"
    git apply "$PATCH_FILE"
    git add \
      packages/web-core/src/shared/hooks/useFeishuBots.ts \
      packages/web-core/src/pages/workspaces/FeishuBindingDialog.tsx
    git commit -m "Reapply local Feishu binding refresh fix"
    return 0
  fi

  fail "本地飞书修复补丁无法自动应用：$PATCH_FILE"
}

main() {
  ensure_repo
  cd "$REPO"
  ensure_clean_tracked

  if in_merge; then
    say "检测到上次 merge 卡住，继续自动收口"
    resolve_known_merge_conflicts
  else
    say "切换到分支 ${BRANCH}"
    git switch "$BRANCH"
    say "抓取 ${UPSTREAM_REMOTE}/${UPSTREAM_BRANCH}"
    git fetch "$UPSTREAM_REMOTE" --tags
    say "合并 ${UPSTREAM_REMOTE}/${UPSTREAM_BRANCH}"
    if git merge --no-edit "${UPSTREAM_REMOTE}/${UPSTREAM_BRANCH}"; then
      say "上游合并完成"
    else
      if in_merge; then
        resolve_known_merge_conflicts
      else
        fail "merge 失败，且未进入可恢复冲突状态。"
      fi
    fi
  fi

  apply_local_patch

  if [ "$SKIP_PUSH" = "1" ]; then
    say "按要求跳过 push"
    return 0
  fi

  say "推送到 origin/${BRANCH}"
  git push origin "$BRANCH"
  say "全部完成"
}

main "$@"

# Feishu Group Binding Uniqueness Design

## Goal

Make Feishu binding ownership match the product rule you confirmed during live validation:

- one Feishu group can bind to only one active workspace
- one Feishu bot can still serve many groups
- binding attempts must fail with a clear message instead of silently switching ownership

## Current problem

The earlier implementation only converged duplicate bindings inside the same `bot_id + target_id` pair. That was too narrow. It allowed two different bot-target rows that point to the same Feishu group identity to remain active across different workspaces, which made the binding model feel inconsistent and confusing during setup.

## Chosen approach

Use the Feishu group identity (`open_chat_id` or `chat_id`) as the uniqueness boundary for active bindings.

### Service layer

- Load the target being bound or re-enabled
- Find other enabled bindings that resolve to the same group identity
- Reject the operation if any other active binding exists
- Include the conflicting workspace name in the error message

### Runtime safety

- Keep the narrow dispatcher self-heal only for exact legacy duplicate rows on the same bot target
- Do not silently auto-disable other workspaces during normal binding flows

### Front-end behaviour

- Surface the backend conflict message in the binding dialog so the user sees why the bind failed

## Why this approach

- It matches the user-facing rule exactly
- It avoids hidden ownership changes
- It keeps the implementation small and local to existing Feishu binding flows
- It still protects live chat from older bad data that may already exist in the database

#!/usr/bin/env bash
# Ironclad workspace architecture guardrails for anomalyco/opencode-rune.
# Enforces progressive disclosure, resolved acyclic crate graph, and effective App field ratchet:
#   1. Strict downward crate dependency layers (resolved transitively via `cargo metadata`, normal + build; zero cycles)
#   2. Zero cross-feature crate dependencies under features/* (resolved transitively; decouple via opencode-kernel / Luau)
#   2b. Core-crate downward isolation (resolved transitively; temporary exceptions in scripts/arch-check-exceptions.txt)
#   3. Core crates (opencode-kernel, opencode-platform, opencode-runtime, opencode-search, opencode-ui) forbid unsafe_code
#   4. apps/desktop/src/ root stays clean (<= 10 top-level .rs files; domains live in subfolders)
#   5. struct App effective field-count ratchet (counts through shell/app_state.rs and any Tracked wrappers; ceilings only go down)
#   6. apps/web depends directly on opencode-desktop (normal dep) and never duplicates features/* deps (resolved via `cargo metadata`)
#   7. No automated tests (#[test], #[cfg(...test...)], tests.rs, *_tests.rs, *_test.rs, tests/) in tracked sources (AGENTS.md)
#   8. Hard <= 800-line cap (no exceptions) on every tracked .rs file in apps/desktop/, features/,
#      and hand-written workspace crates/ (excluding crates/opencode-plugin-spec/src/generated/ and crates/browser-pane/).
#   9. Pure feature crates under features/*/src/lib.rs forbid unsafe_code (outside explicit FFI/GPU allowlist)
#  10. Zero `use crate::*;` ambient crate-root wildcard imports in apps/desktop/src/
#  11. Feature crate naming (opencode-feature-*), workspace manifest completeness, single Rune rev pin, and --!strict built-in Luau plugins
# Usage: scripts/arch-check.sh
set -euo pipefail
ROOT="$(git rev-parse --show-toplevel)"
cd "$ROOT"
exec bash "$ROOT/scripts/guardrails/run-js.sh" "$ROOT/scripts/guardrails/arch-check.mjs" "$@"

# Handoff: Vertical Accordion Mode for Paneru

**Date:** 2026-04-03
**Branch:** `main`
**Session goal:** Implement AeroSpace-style vertical accordion mode for paneru's stack columns, where only the focused window is fully visible and others collapse to configurable-height slivers.

## Status

In Progress — all code changes are written but **not compiled or tested** (no Rust toolchain available in this environment). Needs verification on macOS with cargo.

## What was accomplished

- Designed the feature (spec at `docs/superpowers/specs/2026-04-03-accordion-mode-design.md`)
- Wrote the implementation plan (at `docs/superpowers/plans/2026-04-03-accordion-mode.md`)
- Implemented all 6 plan tasks across 4 files (365 insertions, 54 deletions)
- Updated CONFIGURATION.md with the new option and keybinding

## Mental model

- **Approach taken:** Added a `StackMode` enum (`Split` | `Accordion`) as a second field on `Column::Stack`. This minimises churn: most match arms just add `_` for the mode field. Only `relative_positions` (layout algorithm), `equalize_column` (guard), and the new `toggle_accordion_handler` inspect the mode. The focused entity is passed into `relative_positions` from the calling Bevy systems via a `FocusedMarker` query, avoiding duplicating focus state inside `LayoutStrip`.
- **Alternatives considered:**
  - **New `Column::Accordion` variant** — rejected because it would require a new arm in every `Column` match across the codebase (high churn, high risk of missed cases).
  - **ECS component (`AccordionMarker`)** — rejected because it breaks the architectural invariant that layout state lives in `LayoutStrip`, and would be fragile when focus changes.
- **Constraints / assumptions:**
  - Keyboard-only; no tab bar UI, no mouse click-to-switch (out of scope).
  - Instant snapping, no animation (matches AeroSpace behaviour).
  - When focus leaves the accordion column entirely, the first item expands by default. Per-column last-focused memory is deferred (paneru issue #164).
  - Overflow guard: if `sliver_height * n > viewport_height`, the focused window is clamped to `MIN_WINDOW_HEIGHT` (200px).
- **Gotchas:**
  - `relative_positions` is called from two Bevy systems (`layout_sizes_changed`, `layout_strip_changed`) plus ~11 test call sites. All needed the two new parameters (`focused_entity`, `accordion_sliver_height`).
  - The `position_layout_windows` system uses `matches!(col, Column::Stack(..))` (two dots, not underscore) for the matches macro.
  - `LayoutStrip::stack()` has three constructor paths: when the target is `Single` or `Tabs`, new stacks get `StackMode::default()` (Split). When the target is already a `Stack`, its existing mode is preserved.
  - Git identity (`user.name`/`user.email`) is not configured in this environment, so no commits were made.

## Technical state

- **Uncommitted changes:** Yes. All implementation changes are unstaged modifications. The design spec is staged as a new file. The plan directory is untracked.
- **Failing tests:** Unknown. Cannot compile (no Rust toolchain on this Linux machine; paneru targets macOS).
- **Environment notes:** This is a macOS tiling window manager project cloned on a Linux host. Compilation requires `cargo` with a macOS target or an actual macOS machine. The project uses Bevy ECS, `objc2`, and AppKit, none of which will compile on Linux.

## File map

| File | Change | Why |
|------|--------|-----|
| `src/ecs/layout.rs` | modified | `StackMode` enum, `accordion_heights()` function, `Column::Stack` second field, all match arm updates, `relative_positions` accordion branch with `focused_entity`/`accordion_sliver_height` params, `FocusedMarker` queries in `layout_sizes_changed`/`layout_strip_changed`, 4 new tests |
| `src/commands.rs` | modified | `Operation::ToggleAccordion` variant, `toggle_accordion_handler` function, handler registration, `StackMode` import, equalize guard for accordion mode, match arm updates |
| `src/config.rs` | modified | `accordion_sliver_height: Option<u16>` field in `MainOptions`, `Config::accordion_sliver_height()` accessor, `"accordion"` arm in `parse_operation` |
| `CONFIGURATION.md` | modified | Documented `accordion_sliver_height` option and `window_accordion` keybinding |
| `docs/superpowers/specs/2026-04-03-accordion-mode-design.md` | created | Design spec (approach, data model, algorithm, commands, config, edge cases) |
| `docs/superpowers/plans/2026-04-03-accordion-mode.md` | created | Step-by-step implementation plan with 6 tasks |
| `docs/agents/handoff/2026-04-03-001-accordion-mode.md` | created | This handoff document |

## Next steps

- [ ] **Configure git identity** and commit all changes (suggest one commit per logical unit, or a single feature commit)
- [ ] **Compile on macOS:** run `cargo build` and fix any compilation errors. The changes were written without compiler feedback, so typos or borrow-checker issues are possible.
- [ ] **Run tests:** `cargo test --lib` should pass all existing tests plus the 4 new ones (`test_accordion_heights`, `test_accordion_layout_positioning`, `test_accordion_stack_unstack_roundtrip`, `test_accordion_three_windows_middle_focused`)
- [ ] **Manual testing on macOS:** stack 2-3 windows, bind `window_accordion` to a key, toggle accordion mode, verify N/S focus cycling expands the correct window instantly
- [ ] **Verify edge cases:** toggle accordion on a `Single` column (should be no-op), equalize on an accordion stack (should be no-op), unstack from an accordion column (should cleanly degrade to `Single`)
- [ ] **Optional:** configure `accordion_sliver_height` in TOML and verify it takes effect (default is 30px)
- [ ] **Optional:** open a PR against `karinushka/paneru` if contributing upstream

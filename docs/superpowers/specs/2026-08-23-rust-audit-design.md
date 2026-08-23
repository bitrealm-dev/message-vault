# Audit product Rust and define a Rust doc style guide

**Date:** 2026-08-23  
**Status:** Approved for planning

## Context

The docs pipeline is live. Workspace rustdoc publishes at `/vault/developer/rustdoc/` on bitrealm.io (#86), the utoipa HTTP catalog at `/vault/developer/rustdoc/http/`, and CLI pages are generated from clap. The surface is public; the content behind it is uneven.

- Rustdoc coverage is not enforced anywhere. No `missing_docs` lint exists in `crates/` or `src-tauri/`.
- The OpenAPI catalog has missing summaries (`GET /health`), summaries that echo the route (`` `GET /v1/account/api-tokens` ``), and `# Errors` rustdoc sections leaking into description fields. Tag descriptions are one-liners.
- Code health signals: 82k lines across ~26 product crates, 13 `unsafe` sites, no clippy gate in CI (warnings were cleared in #88), and very large files: `server.rs` (3,452 lines), `import.rs` (3,039), `vault-push/src/run.rs` (2,880), `contacts_api.rs` (2,228), `mms_enc.rs` (2,021).
- The user wants product Rust cleaned up (deficiencies, duplication, best practices) and rustdoc + HTTP docs rewritten to be developer-facing: plain, concrete, not "AI agent like".

Cleaning up 26 crates and rewriting their docs is too large for one project. This spec is the first sub-project: a read-only audit that produces the findings report and the style guide. Follow-up projects fix code and rewrite docs per crate group, guided by the report.

## Goal

A committed, evidence-backed audit report that tells the user exactly what is wrong in product Rust, how severe it is, and what to do about it. A committed Rust doc style guide that defines the new voice for rustdoc comments and utoipa annotations, backed by good and bad examples from this codebase. Nothing else changes.

## Non-goals

- Fixing any finding, rewriting any doc comment, or refactoring any code (follow-up projects)
- Auditing `crates/message-vault-io-gui` (legacy Slint), `web/`, or `web-next/`
- Editing Starlight prose pages (they already have voice conventions)
- Bumping dependencies (unused or duplicate dependencies are reported as findings only)
- Enabling lint gates (`missing_docs`, clippy in CI) — those are follow-up decisions driven by findings
- Changing HTTP behavior, the OpenAPI dump, or the docs pipeline

## Decisions

1. **Read-only audit.** Nothing under `crates/`, `src-tauri/`, or `tests/` is modified. Only the two deliverables in this spec are written.
2. **Scope.** Workspace crates except the legacy Slint GUI, plus `src-tauri`.
3. **Nine audit dimensions.** Docs coverage, docs quality, HTTP docs (utoipa), duplication, structure, `unsafe`, error handling, API design, best practices.
4. **Severities.** `high`, `medium`, `low`. Every finding carries `file:line` evidence and a suggestion.
5. **Style guide ships with the audit.** The audit defines the voice once; follow-up doc-rewrite projects follow it instead of re-deriving it.
6. **Workflow execution.** One Workflow run: Stage 1 find (6 agents), Stage 2 adversarial verify (6 agents), Stage 3 completeness critic + synthesis (2 agents). Mechanical signals run inline before the workflow and are handed to the agents as evidence.
7. **Verification bar.** Every `high` finding must survive adversarial verification. After the workflow, every `high` duplication finding and at least 20% of the other `high` findings (rounded up, picked to spread across groups) are re-checked by reading the cited lines before the report is committed.
8. **Style guide rules need examples.** A rule enters the guide only when at least two concrete good or bad examples from this codebase support it.
9. **Report location.** `docs/superpowers/reports/2026-08-23-rust-audit.md`. Style guide at `docs/src/content/docs/vault/developer/rustdoc-style.md`, published at `/vault/developer/rustdoc-style/`, linked from Contributing.

## Architecture

**Rubric.** The nine dimensions and what each looks for:

| Dimension | What it looks for |
|-----------|-------------------|
| Docs: coverage | `pub` items with no doc comment; modules missing `//!` intros; which crates lack `#![warn(missing_docs)]` |
| Docs: quality | Obtuse or clever phrasing, filler adjectives, no examples on non-obvious APIs, broken intra-doc links, stale docs |
| HTTP docs | utoipa summaries that echo the route or are missing, `# Errors` rustdoc sections leaking into OpenAPI descriptions, thin tag descriptions |
| Duplication | Cross-crate (the seven exporters share emit and attachment patterns; `attachments.rs` exists in several), intra-crate copy-paste, duplicated test scaffolding |
| Structure | Oversized files and functions, tangled module boundaries |
| `unsafe` | Every `unsafe` site checked for soundness and a safety comment |
| Error handling | anyhow vs thiserror consistency; the deliberate no-`anyhow` rule in `message-vault-io-core`; string-mapped errors at crate edges |
| API design | `pub` surface leakage, re-export hygiene, `pub(crate)` misuse, public types that should be private |
| Best practices | Clippy not gated in CI, no `missing_docs` enforcement, unused or duplicate dependencies, workspace conventions |

Severity meanings: `high` — soundness, security, or correctness risk, or a major documentation failure on public API. `medium` — maintainability, moderate duplication, doc gaps that mislead. `low` — style and nits.

Three phases. Total agent count: 14 (6 find, 6 verify, 2 synthesize).

**Phase 0 — mechanical signals (inline).** Run clippy over the workspace and `src-tauri`, record residual warnings. Scan for `pub` items lacking doc comments. Extract the 13 `unsafe` sites. Build a file-size histogram. Compare dependencies in the `Cargo.toml` files against actual `use` statements to find unused dependencies. Pull every utoipa summary and description out of `docs/src/assets/openapi.json`. Each result goes into the workflow as evidence.

**Phase 1 — find.** Six parallel read-only agents:

| Agent | Covers | Focus |
|-------|--------|-------|
| libs | `crates/libs/*` (11 crates) | Rubric. `message-ir` is the model every exporter writes. |
| exporters-core | `crates/exporters/*` + `crates/core/message-vault-io-core` | Rubric. Shared pipeline duplication. |
| cli-seed | `crates/cli/*` + `crates/vault/demo-seed` | Rubric. |
| server | `crates/vault/server` | Rubric. utoipa annotations. The largest files. |
| src-tauri | `src-tauri/src` | Rubric. Command wrappers around exporter and push/pull crates. |
| duplication | cross-crate | Exporters against each other and against libs. Helpers that should be extracted. |

Each agent returns structured findings plus its five best and five worst doc comments (for the style guide).

**Phase 2 — verify.** Six parallel adversarial verifiers, one per group. For each finding that touches that group's files: the evidence supports the claim, the severity is right, it is not a false positive, it is not already fixed on this branch. Findings that fail are dropped, not published.

**Phase 3 — synthesize.** One completeness critic ("which dimension or crate got thin coverage") and one synthesis agent that merges verified findings into category and severity buckets and drafts follow-up project groupings.

**After the workflow.** The main loop writes the final report and the style guide, re-checks the sampled `high` findings, then commits both files.

Finding schema: `category`, `severity`, `crate`, `file:line`, `title`, `evidence`, `suggestion`.

## Data flow

1. Run the Phase 0 commands and capture the output.
2. Launch the workflow with the rubric and the Phase 0 evidence as input.
3. The workflow returns verified findings, completeness notes, good and bad doc examples, and draft follow-up groupings.
4. The main loop writes the two deliverable files, re-checks sampled findings, and commits.

## Error handling

- If an agent dies or returns nothing, its group is re-run once. If verification cannot cover a finding, the finding is dropped rather than published unverified.
- If a finding's evidence is not reproducible from the cited `file:line`, verification drops it.
- If the workflow fails entirely, no report is written. The user sees the failure, not a partial report.

## Testing and verification of the audit

- Every `high` finding is verified for claim and severity.
- Main-loop re-check: every `high` duplication finding and a sample of other `high` findings are re-read at the cited lines before commit.
- Mechanical findings are reproducible from the commands recorded in the report.
- Every style guide rule cites at least two codebase examples.
- `cd docs && npm run check && npm run build` still passes with the new style guide page.

## What changes

| Path | Change |
|------|--------|
| `docs/superpowers/reports/2026-08-23-rust-audit.md` | New findings report |
| `docs/src/content/docs/vault/developer/rustdoc-style.md` | New style guide page |
| `docs/src/content/docs/vault/developer/contributing.md` | One link to the style guide |
| `docs/astro.config.mjs` | Sidebar entry for the style guide (Developer section) |

## Verification

- The report has findings in all nine dimensions, each with evidence.
- Every `high` finding passed adversarial verification and the sampled re-check.
- The style guide covers: the first-sentence rule, module `//!` intros, when examples are required, a banned-phrasing list, utoipa summary and description rules (summary says what the route does; description says when and why; no echoed routes; no `# Errors` leakage), and the coverage rule (every `pub` item documented).
- `cd docs && npm run check && npm run build` passes.
- No file outside the table above changed.

## Success criteria

- The user can decide follow-up work from the report without re-reading code.
- Follow-up projects cite the style guide instead of re-deriving the voice.
- The audit changed no product code.

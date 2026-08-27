# Product Rust Audit and Doc Style Guide Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Produce a committed, evidence-backed audit report of product Rust (workspace minus Slint GUI, plus `src-tauri`) and a committed Rust doc style guide backed by codebase examples.

**Architecture:** Phase 0 collects mechanical signals (clippy, missing-docs heuristic, unsafe sites, largest files, utoipa extraction) into `.tmp/rust-audit/`. One Workflow run then finds (6 agents), adversarially verifies (6 agents), and synthesizes (2 agents). The main loop re-checks sampled `high` findings, writes the report and the style guide page, and commits.

**Tech Stack:** cargo clippy, Workflow orchestration (find → verify → synthesize), Astro Starlight (`docs/`), git.

**Spec:** `docs/superpowers/specs/2026-08-23-rust-audit-design.md`

## Global Constraints

- Read-only audit: nothing under `crates/`, `src-tauri/`, or `tests/` is modified. Only the files in this plan's file map are written.
- Scope: workspace crates except `crates/message-vault-io-gui`, plus `src-tauri`. Excluded: `web/`, `web-next/`, Starlight prose pages (except the files named below).
- Severity meanings: `high` — soundness, security, or correctness risk, or a major documentation failure on public API. `medium` — maintainability, moderate duplication, doc gaps that mislead. `low` — style and nits.
- Every finding carries `file:line` evidence and a one-sentence suggestion.
- Verification bar: every `high` finding must pass adversarial verification. After the workflow, every `high` duplication finding and at least 20% of the other `high` findings (rounded up, spread across groups) are re-checked at the cited lines before the report is committed. Findings that fail verification are dropped, not published.
- Style guide rules: a rule enters the guide only with at least two concrete good/bad examples from this codebase.
- No dependency bumps. Unused/duplicate dependencies are reported as findings only.
- Docs voice for the style guide page: short sentences, concrete commands, no "we" / "us" / "our".
- Commit on the current branch (`docs/clap-and-rustdoc-starlight`). Never commit to `main`.
- `.tmp/` is gitignored. Nothing under `.tmp/rust-audit/` is committed.
- Workflow agent budget: 14 agents (6 find + 6 verify + 2 synthesize).

## File map

| File | Responsibility |
|------|----------------|
| `.tmp/rust-audit/clippy-workspace.txt` | Phase 0 clippy output, workspace (scratch) |
| `.tmp/rust-audit/clippy-src-tauri.txt` | Phase 0 clippy output, src-tauri (scratch) |
| `.tmp/rust-audit/missing-docs-heuristic.txt` | Phase 0 pub-items-without-docs scan (scratch) |
| `.tmp/rust-audit/unsafe-sites.txt` | Phase 0 unsafe site list (scratch) |
| `.tmp/rust-audit/largest-files.txt` | Phase 0 file-size histogram (scratch) |
| `.tmp/rust-audit/extract-openapi.py` | Phase 0 utoipa extraction script (scratch) |
| `.tmp/rust-audit/utoipa-summaries.txt` | Phase 0 utoipa summary/description report (scratch) |
| `.tmp/rust-audit/workflow-result.json` | Workflow return value saved to disk (scratch) |
| `.tmp/rust-audit/recheck.log` | Main-loop spot-check notes (scratch) |
| `docs/superpowers/reports/2026-08-23-rust-audit.md` | The committed findings report |
| `docs/src/content/docs/vault/developer/rustdoc-style.md` | The committed style guide page |
| `docs/src/content/docs/vault/developer/contributing.md` | Gains one link to the style guide |
| `docs/astro.config.mjs` | Gains the sidebar entry for the style guide |

---

### Task 1: Phase 0 mechanical signals

**Files:**
- Create: everything under `.tmp/rust-audit/` except `workflow-result.json` and `recheck.log`

**Interfaces:**
- Produces: evidence files under `.tmp/rust-audit/` that Task 2's workflow agents read. File names are fixed; agents depend on them.

- [ ] **Step 1: Create the scratch directory and run clippy**

```bash
mkdir -p /home/mbeisser/repo/message-vault/.tmp/rust-audit
cd /home/mbeisser/repo/message-vault
cargo clippy --workspace 2>&1 | tee .tmp/rust-audit/clippy-workspace.txt
cargo clippy --manifest-path src-tauri/Cargo.toml 2>&1 | tee .tmp/rust-audit/clippy-src-tauri.txt
```

Expected: both files exist. Warnings may be present; that is evidence, not a failure. Exit codes may be non-zero on warnings — that is fine here.

- [ ] **Step 2: Scan for `pub` items without an adjacent doc comment (heuristic)**

```bash
cd /home/mbeisser/repo/message-vault
find crates src-tauri/src -name '*.rs' \
  ! -path '*/target/*' \
  ! -path '*/message-vault-io-gui/*' \
  ! -name 'main.rs' ! -name 'lib.rs' \
  -print0 | sort -z | xargs -0 awk '
  /^[[:space:]]*pub (fn|struct|enum|trait|const|type|mod)/ {
    if (prev !~ /^[[:space:]]*\/\/\//) print FILENAME ":" FNR ": " $0
  }
  { if ($0 !~ /^[[:space:]]*$/) prev=$0 }
' > .tmp/rust-audit/missing-docs-heuristic.txt
wc -l .tmp/rust-audit/missing-docs-heuristic.txt
```

Expected: a line count. The heuristic flags `pub` items whose previous non-blank line is not a `///` doc comment. False positives are expected (doc comments with blank lines between them and the item); agents judge.

- [ ] **Step 3: List `unsafe` sites and the largest files**

```bash
cd /home/mbeisser/repo/message-vault
grep -rn "unsafe" crates src-tauri/src --include='*.rs' \
  | grep -v 'crates/message-vault-io-gui' | grep -v '/target/' \
  > .tmp/rust-audit/unsafe-sites.txt
find crates src-tauri/src -name '*.rs' \
  ! -path '*/target/*' ! -path '*/message-vault-io-gui/*' \
  -exec wc -l {} + | sort -rn | head -40 > .tmp/rust-audit/largest-files.txt
cat .tmp/rust-audit/unsafe-sites.txt
```

Expected: roughly 13 `unsafe` lines (including `unsafe impl` and safety comments). `largest-files.txt` lists 40 files with line counts, largest first.

- [ ] **Step 4: Extract utoipa summaries and descriptions**

Write `.tmp/rust-audit/extract-openapi.py`:

```python
import json, sys

doc = json.load(open(sys.argv[1]))
print("## Tags")
for t in doc.get("tags", []):
    print(f"- {t['name']}: {t.get('description') or '(no description)'}")

print("\n## Operations")
for path, ops in doc.get("paths", {}).items():
    for method, op in ops.items():
        summary = op.get("summary")
        desc = (op.get("description") or "").strip()
        flags = []
        if not summary:
            flags.append("NO-SUMMARY")
        elif summary.strip() == f"`{method.upper()} {path}`":
            flags.append("ECHOED-ROUTE-SUMMARY")
        if "# Errors" in desc:
            flags.append("ERRORS-SECTION-IN-DESCRIPTION")
        if desc == "":
            flags.append("NO-DESCRIPTION")
        print(f"{method.upper()} {path} | summary={summary!r} | desc={desc[:100]!r} | {' '.join(flags)}")
```

Run:

```bash
cd /home/mbeisser/repo/message-vault
python3 .tmp/rust-audit/extract-openapi.py docs/src/assets/openapi.json > .tmp/rust-audit/utoipa-summaries.txt
grep -c "NO-SUMMARY" .tmp/rust-audit/utoipa-summaries.txt || true
grep -c "ECHOED-ROUTE-SUMMARY" .tmp/rust-audit/utoipa-summaries.txt || true
grep -c "ERRORS-SECTION-IN-DESCRIPTION" .tmp/rust-audit/utoipa-summaries.txt || true
```

Expected: the counts confirm the problem is real (missing summaries, echoed-route summaries, `# Errors` leakage). Record the counts; the report cites them.

- [ ] **Step 5: Confirm the evidence files exist**

```bash
ls -la /home/mbeisser/repo/message-vault/.tmp/rust-audit/
```

Expected: seven files from Steps 1–4, all non-empty except possibly `clippy-src-tauri.txt` (an empty file is still evidence).

No commit: `.tmp/` is gitignored.

---

### Task 2: Run the audit Workflow

**Files:**
- Create: `.tmp/rust-audit/workflow-result.json` (the saved Workflow return value)

**Interfaces:**
- Consumes: evidence files from Task 1
- Produces: `workflow-result.json` containing `{ findings, critic, synthesis }`:
  - `findings` — verified finding objects: `{ category, severity, crate, file_line, title, evidence, suggestion, group }`
  - `critic` — `{ gaps: string[], coverage_notes: string }`
  - `synthesis` — `{ stats: object, report_draft: string, style_rules: [{ rule, examples: [{ file_line, quote, note }] }], followup_groups: [{ name, crates, rationale, finding_count }] }`

- [ ] **Step 1: Run the workflow**

Execute this script with the Workflow tool (it runs in the background; you are re-invoked when it completes):

```js
export const meta = {
  name: 'rust-audit',
  description: 'Audit product Rust: find, verify, and synthesize evidence-backed findings',
  phases: [
    { title: 'Find', detail: '6 read-only agents: 5 crate groups + cross-crate duplication' },
    { title: 'Verify', detail: 'adversarial verification per group partition' },
    { title: 'Synthesize', detail: 'completeness critic + report/style synthesis' },
  ],
}

const EVIDENCE_DIR = '/home/mbeisser/repo/message-vault/.tmp/rust-audit'

const DIMENSIONS = [
  'docs-coverage: pub items with no doc comment; modules missing //! intros; crates lacking #![warn(missing_docs)]',
  'docs-quality: obtuse or clever phrasing, filler adjectives, no examples on non-obvious APIs, broken intra-doc links, stale docs',
  'http-docs: utoipa summaries that echo the route or are missing; # Errors rustdoc sections leaking into OpenAPI descriptions; thin tag descriptions',
  'duplication: cross-crate (exporters share emit and attachment patterns; attachments.rs exists in several), intra-crate copy-paste, duplicated test scaffolding',
  'structure: oversized files and functions, tangled module boundaries',
  'unsafe: every unsafe site checked for soundness and a safety comment',
  'error-handling: anyhow vs thiserror consistency; the deliberate no-anyhow rule in message-vault-io-core; string-mapped errors at crate edges',
  'api-design: pub surface leakage, re-export hygiene, pub(crate) misuse, public types that should be private',
  'best-practices: clippy not gated in CI, no missing_docs enforcement, unused or duplicate dependencies, workspace conventions',
].join('\n')

const SEVERITIES = [
  'high - soundness, security, or correctness risk, or a major documentation failure on public API',
  'medium - maintainability, moderate duplication, doc gaps that mislead',
  'low - style and nits',
].join('\n')

const FINDING = {
  type: 'object',
  additionalProperties: false,
  properties: {
    category: { type: 'string', enum: ['docs-coverage', 'docs-quality', 'http-docs', 'duplication', 'structure', 'unsafe', 'error-handling', 'api-design', 'best-practices'] },
    severity: { type: 'string', enum: ['high', 'medium', 'low'] },
    crate: { type: 'string' },
    file_line: { type: 'string' },
    title: { type: 'string' },
    evidence: { type: 'string' },
    suggestion: { type: 'string' },
    group: { type: 'string' },
  },
  required: ['category', 'severity', 'crate', 'file_line', 'title', 'evidence', 'suggestion', 'group'],
}

const DOC_EXAMPLE = {
  type: 'object',
  additionalProperties: false,
  properties: { file_line: { type: 'string' }, quote: { type: 'string' }, note: { type: 'string' } },
  required: ['file_line', 'quote', 'note'],
}

const FINDER_SCHEMA = {
  type: 'object',
  additionalProperties: false,
  properties: {
    findings: { type: 'array', items: FINDING },
    good_docs: { type: 'array', items: DOC_EXAMPLE, maxItems: 5 },
    bad_docs: { type: 'array', items: DOC_EXAMPLE, maxItems: 5 },
  },
  required: ['findings', 'good_docs', 'bad_docs'],
}

const VERIFY_SCHEMA = {
  type: 'object',
  additionalProperties: false,
  properties: {
    verified: { type: 'array', items: FINDING },
    dropped: {
      type: 'array',
      items: {
        type: 'object',
        additionalProperties: false,
        properties: { finding: FINDING, reason: { type: 'string' } },
        required: ['finding', 'reason'],
      },
    },
  },
  required: ['verified', 'dropped'],
}

const CRITIC_SCHEMA = {
  type: 'object',
  additionalProperties: false,
  properties: {
    gaps: { type: 'array', items: { type: 'string' } },
    coverage_notes: { type: 'string' },
  },
  required: ['gaps', 'coverage_notes'],
}

const SYNTH_SCHEMA = {
  type: 'object',
  additionalProperties: false,
  properties: {
    stats: { type: 'object' },
    report_draft: { type: 'string' },
    style_rules: {
      type: 'array',
      items: {
        type: 'object',
        additionalProperties: false,
        properties: { rule: { type: 'string' }, examples: { type: 'array', items: DOC_EXAMPLE, minItems: 2 } },
        required: ['rule', 'examples'],
      },
    },
    followup_groups: {
      type: 'array',
      items: {
        type: 'object',
        additionalProperties: false,
        properties: { name: { type: 'string' }, crates: { type: 'string' }, rationale: { type: 'string' }, finding_count: { type: 'number' } },
        required: ['name', 'crates', 'rationale'],
      },
    },
  },
  required: ['stats', 'report_draft', 'style_rules', 'followup_groups'],
}

function finderPrompt(label, base, focus, group) {
  return [
    'You are an auditor for a READ-ONLY Rust code audit. Group: ' + label + '.',
    'Files to audit: ' + base + '.',
    'READ-ONLY: never modify, create, or delete any file. Use Read and Grep only. Do not run cargo build, cargo test, cargo clippy, or any command that writes files.',
    'Phase 0 evidence files are in ' + EVIDENCE_DIR + ' (clippy output, missing-docs heuristic, unsafe sites, largest files, utoipa summary extraction). Read the ones that apply and use them as leads; you do the judging.',
    '',
    'Audit dimensions and what each looks for:',
    DIMENSIONS,
    '',
    'Severity meanings:',
    SEVERITIES,
    '',
    focus,
    '',
    'Finding rules:',
    '- file_line must be a real path and line that exists, for example crates/vault/server/src/server.rs:1234. Verify every cited line before reporting it.',
    '- evidence: quote or describe exactly what is at that line, under 40 words.',
    '- suggestion: one concrete action, one sentence.',
    '- Report only real problems. When a dimension is clean, report nothing for it.',
    '- Set group to "' + group + '" on every finding.',
    '- Also collect up to 5 best and up to 5 worst doc comments (//! or ///) you see: file_line, a short quote, and a one-sentence note on why it is good or bad.',
  ].join('\n')
}

function verifyPrompt(label, findingsJson) {
  return [
    'You are an adversarial verifier for a READ-ONLY Rust audit. Group: ' + label + '.',
    'Your job is to REFUTE. For each finding in the JSON below, open the cited file_line with Read and check:',
    '1. The evidence is really at that location.',
    '2. The claim follows from the evidence.',
    '3. The severity matches: ' + SEVERITIES,
    '4. It is not already fixed on the current branch.',
    'Move every finding that fails any check to dropped with a one-line reason. Keep the rest in verified, unchanged. Default to dropping when you are uncertain. Do not invent new findings, do not modify files, do not run cargo.',
    'Findings (JSON):',
    findingsJson,
  ].join('\n')
}

const GROUPS = [
  { key: 'libs', base: '/home/mbeisser/repo/message-vault/crates/libs', focus: 'message-ir is the model every exporter writes; its docs matter most. Check cross-crate reuse between the libs.' },
  { key: 'exporters-core', base: '/home/mbeisser/repo/message-vault/crates/exporters and /home/mbeisser/repo/message-vault/crates/core/message-vault-io-core', focus: 'Watch for pipeline and attachment handling duplication across the seven exporters and message-vault-io-core.' },
  { key: 'cli-seed', base: '/home/mbeisser/repo/message-vault/crates/cli and /home/mbeisser/repo/message-vault/crates/vault/demo-seed', focus: 'CLI parsing and clap doc comments; demo-seed correctness.' },
  { key: 'server', base: '/home/mbeisser/repo/message-vault/crates/vault/server', focus: 'Judge utoipa annotations too: summaries that echo the route, missing summaries, # Errors leakage into descriptions. The largest files are server.rs, import.rs, contacts_api.rs, conversations_api.rs, export_api.rs, auth.rs, guest_clone.rs, reset_demo.rs.' },
  { key: 'src-tauri', base: '/home/mbeisser/repo/message-vault/src-tauri/src', focus: 'Command wrappers around exporter and push/pull crates; watch for thin duplication with crates/cli.' },
]

phase('Find')
const found = await parallel(
  GROUPS.map((g) => () =>
    agent(finderPrompt(g.key, g.base, g.focus, g.key), {
      label: 'find:' + g.key,
      phase: 'Find',
      schema: FINDER_SCHEMA,
    }),
  ).concat([
    () =>
      agent(
        finderPrompt(
          'duplication',
          '/home/mbeisser/repo/message-vault/crates/exporters, /home/mbeisser/repo/message-vault/crates/libs, /home/mbeisser/repo/message-vault/crates/core, /home/mbeisser/repo/message-vault/crates/cli',
          'Cross-crate comparison only. Compare the seven exporters against each other and against the libs and message-vault-io-core. Report near-identical code blocks, duplicated test scaffolding, and helpers that should be extracted into a lib. Cite the primary file in file_line. Set group to "duplication" on every finding.',
          'duplication',
        ),
        { label: 'find:duplication', phase: 'Find', schema: FINDER_SCHEMA },
      ),
  ]),
)

const allFindings = []
const goodDocs = []
const badDocs = []
for (const r of found) {
  if (!r) continue
  allFindings.push(...r.findings)
  goodDocs.push(...r.good_docs)
  badDocs.push(...r.bad_docs)
}

const pathOf = (f) => (f.file_line || '') + ' ' + (f.crate || '')
const partition = { libs: [], 'exporters-core': [], 'cli-seed': [], server: [], 'src-tauri': [], duplication: [] }
const noHome = []
for (const f of allFindings) {
  if (f.group === 'duplication') {
    partition.duplication.push(f)
    continue
  }
  const p = pathOf(f)
  let home = null
  if (p.includes('crates/libs/')) home = 'libs'
  else if (p.includes('crates/exporters/') || p.includes('crates/core/')) home = 'exporters-core'
  else if (p.includes('crates/cli/') || p.includes('crates/vault/demo-seed')) home = 'cli-seed'
  else if (p.includes('crates/vault/server')) home = 'server'
  else if (p.includes('src-tauri')) home = 'src-tauri'
  if (home) partition[home].push(f)
  else noHome.push(f)
}
log(`partition: ${partition.libs.length}/${partition['exporters-core'].length}/${partition['cli-seed'].length}/${partition.server.length}/${partition['src-tauri'].length}/${partition.duplication.length}; noHome dropped: ${noHome.length}`)

phase('Verify')
const verifyResults = await parallel(
  Object.keys(partition).map((key) => () => {
    const items = partition[key]
    if (items.length === 0) return { verified: [], dropped: [] }
    return agent(verifyPrompt(key, JSON.stringify(items, null, 2)), {
      label: 'verify:' + key,
      phase: 'Verify',
      schema: VERIFY_SCHEMA,
    })
  }),
)

const verifiedFindings = []
let droppedCount = 0
for (const v of verifyResults) {
  if (!v) continue
  verifiedFindings.push(...v.verified)
  droppedCount += v.dropped.length
}
log(`verify: ${verifiedFindings.length} confirmed, ${droppedCount} dropped`)

phase('Synthesize')
const [critic, synthesis] = await parallel([
  () =>
    agent(
      [
        'You are a completeness critic for a Rust audit that just finished.',
        'The audit covered these dimensions:',
        DIMENSIONS,
        '',
        'These crate groups were searched: crates/libs, crates/exporters, crates/core, crates/cli, crates/vault/demo-seed, crates/vault/server, src-tauri, plus a cross-crate duplication pass.',
        'Verified finding counts per group: ' + Object.keys(partition).map((k) => k + '=' + partition[k].length).join(', ') + '; verified ' + verifiedFindings.length + ', dropped ' + droppedCount + '.',
        'Ask: which dimension or crate got thin coverage? What should the next audit pass or a follow-up project look at that this one likely missed? Be concrete; name files or areas.',
        'Return gaps as a list of short strings plus one coverage_notes paragraph.',
      ].join('\n'),
      { label: 'synthesize:critic', phase: 'Synthesize', schema: CRITIC_SCHEMA },
    ),
  () =>
    agent(
      [
        'You synthesize the results of a verified Rust audit into draft deliverables.',
        '',
        'Verified findings (JSON):',
        JSON.stringify(verifiedFindings, null, 2),
        '',
        'Good doc comments collected by the finders (JSON):',
        JSON.stringify(goodDocs, null, 2),
        '',
        'Bad doc comments collected by the finders (JSON):',
        JSON.stringify(badDocs, null, 2),
        '',
        'Produce:',
        '1. stats: counts of findings by severity and by category.',
        '2. report_draft: markdown for the findings report body. Group by category; under each category list findings by severity (high, medium, low). Each finding as a bullet: severity, title, file_line, evidence, suggestion. Keep every finding; do not summarize away details.',
        '3. style_rules: draft rules for the Rust doc style guide. Each rule: one imperative sentence of guidance. Each rule MUST cite at least 2 examples from the good/bad doc lists (file_line + quote). Drop any rule that cannot reach 2 examples. Cover at minimum: the first-sentence rule, module //! intros, when examples are required, banned phrasing, utoipa summary and description rules, and the coverage rule (every pub item documented).',
        '4. followup_groups: propose 3 to 5 follow-up projects (name, crates covered, one-sentence rationale, finding_count), mixing code fixes and doc rewrites per crate group.',
      ].join('\n'),
      { label: 'synthesize:report', phase: 'Synthesize', schema: SYNTH_SCHEMA },
    ),
])

return { findings: verifiedFindings, critic, synthesis }
```

- [ ] **Step 2: Wait for completion**

The workflow runs in the background. A task notification arrives when it finishes. Do not proceed until then. If it fails or times out, re-run once with `resumeFromRunId` after reading the failure output.

- [ ] **Step 3: Save the result to disk**

Save the workflow's return value (the object `{ findings, critic, synthesis }`) as JSON:

```bash
cat > /home/mbeisser/repo/message-vault/.tmp/rust-audit/workflow-result.json <<'EOF'
<paste the full JSON here>
EOF
python3 -c "import json; d=json.load(open('/home/mbeisser/repo/message-vault/.tmp/rust-audit/workflow-result.json')); print(len(d['findings']), 'findings'); print(len(d['synthesis']['style_rules']), 'style rules'); print(len(d['synthesis']['followup_groups']), 'followup groups')"
```

Expected: the counts print. If `workflow-result.json` is empty or unparseable, the paste failed — redo it.

---

### Task 3: Main-loop verification pass

**Files:**
- Create: `.tmp/rust-audit/recheck.log`

**Interfaces:**
- Consumes: `workflow-result.json` from Task 2
- Produces: `recheck.log` — one line per re-checked finding: `PASS <file_line> <title>` or `FAIL <file_line> <title> — <reason>`. A `FAIL` line removes that finding from the report in Task 4.

- [ ] **Step 1: Pick the re-check sample**

From `workflow-result.json` `findings`, select:
- every `high` finding with category `duplication`, and
- at least 20% of the remaining `high` findings, rounded up, spread across the groups (`libs`, `exporters-core`, `cli-seed`, `server`, `src-tauri`), preferring categories with few representatives.

Print the sample as a list of `file_line` values.

- [ ] **Step 2: Re-check each sampled finding**

For each sampled finding, open the cited `file_line` with the Read tool and confirm the evidence is present and the claim holds. Append one line to `recheck.log` per finding (PASS or FAIL with a short reason). A finding whose line moved slightly on this branch (the evidence is one or two lines away) still PASSes — record the real line.

- [ ] **Step 3: Report the re-check result**

Count PASS and FAIL lines:

```bash
grep -c '^PASS' /home/mbeisser/repo/message-vault/.tmp/rust-audit/recheck.log
grep -c '^FAIL' /home/mbeisser/repo/message-vault/.tmp/rust-audit/recheck.log || true
```

Expected: FAIL count is small. If FAIL count is more than 20% of the sample, verification quality is suspect — stop and tell the user before writing the report.

---

### Task 4: Write and commit the findings report

**Files:**
- Create: `docs/superpowers/reports/2026-08-23-rust-audit.md`

**Interfaces:**
- Consumes: `workflow-result.json` and `recheck.log` from Tasks 2–3
- Produces: the committed report. Task 5 links nothing to it; the user reads it directly.

- [ ] **Step 1: Build the final finding list**

Start from `workflow-result.json` `findings` and drop every finding with a FAIL line in `recheck.log`. If a re-check moved a `file_line`, use the corrected line.

- [ ] **Step 2: Write the report**

Create `docs/superpowers/reports/2026-08-23-rust-audit.md` with this structure:

```markdown
# Product Rust audit — 2026-08-23

Scope: workspace crates except `crates/message-vault-io-gui`, plus `src-tauri`. Read-only; no code changed.

## Summary

<counts by severity and by category, in a small table>

## Mechanical signals

Commands and counts anyone can rerun: clippy (workspace + src-tauri), the
missing-docs heuristic, the unsafe-site list, the largest-files list, and the
utoipa extraction (missing / echoed-route / # Errors counts from
`.tmp/rust-audit/utoipa-summaries.txt`).

## Findings by category

<from synthesis.report_draft, edited: findings dropped by re-check are
removed; corrections applied. Every finding keeps severity, title, file_line,
evidence, and suggestion.>

## Follow-up proposals

<synthesis.followup_groups as a table: name, crates, rationale, finding_count>

## Coverage gaps

<critic.gaps as a list, plus critic.coverage_notes as a paragraph>

## Verification

- Every finding above passed adversarial verification (claim, severity,
  not-already-fixed), except any FAIL lines recorded in
  `.tmp/rust-audit/recheck.log` — those were removed.
- Re-checked at the cited lines: <count of PASS lines> findings
  (every high-severity duplication finding plus a sample of other highs).

## Out of scope

Legacy Slint GUI (`crates/message-vault-io-gui`), `web/`, `web-next/`,
Starlight prose pages, dependency bumps.
```

Edit the `report_draft` text while writing: remove any placeholder wording, keep evidence under 40 words per finding, and make sure every finding has `file_line`, `evidence`, and `suggestion`.

- [ ] **Step 3: Self-check the report**

```bash
cd /home/mbeisser/repo/message-vault
grep -nE 'TODO|TBD|XXX|FIXME' docs/superpowers/reports/2026-08-23-rust-audit.md || true
```

Expected: no matches.

- [ ] **Step 4: Commit**

```bash
git add docs/superpowers/reports/2026-08-23-rust-audit.md
git commit -m "docs: add product Rust audit findings report"
```

---

### Task 5: Write the style guide page and wire it in

**Files:**
- Create: `docs/src/content/docs/vault/developer/rustdoc-style.md`
- Modify: `docs/astro.config.mjs` (developerItems, after `'vault/developer/contributing'`)
- Modify: `docs/src/content/docs/vault/developer/contributing.md` (one link)

**Interfaces:**
- Consumes: `synthesis.style_rules` from `workflow-result.json`
- Produces: the published page at `/vault/developer/rustdoc-style/`, linked from the sidebar and Contributing.

- [ ] **Step 1: Write the page**

Create `docs/src/content/docs/vault/developer/rustdoc-style.md`. Frontmatter:

```yaml
---
title: Rust doc style
description: How to write rustdoc comments and HTTP API descriptions that developers can use.
---
```

Body requirements:
- Short intro: this page is the standard for `///` and `//!` comments and utoipa annotations. Workspace rustdoc publishes at `/vault/developer/rustdoc/`; the HTTP catalog at `/vault/developer/rustdoc/http/`.
- One section per rule from `synthesis.style_rules`. Sections must cover at minimum: the first-sentence rule, module `//!` intros, when examples are required, banned phrasing, utoipa summary and description rules (summary says what the route does; description says when and why; no echoed routes; no `# Errors` leakage), and the coverage rule (every `pub` item documented).
- Every rule cites at least two codebase examples (`file:line` plus a short quote), good and bad. Rules with fewer than two examples are dropped.
- Voice: short sentences, no "we" / "us" / "our". Do not use GitHub `> [!TIP]` alerts. Use Starlight `title=""` on code fences.

- [ ] **Step 2: Add the sidebar entry**

In `docs/astro.config.mjs`, in `developerItems`, directly after `'vault/developer/contributing',` add:

```javascript
  'vault/developer/rustdoc-style',
```

- [ ] **Step 3: Link from Contributing**

In `docs/src/content/docs/vault/developer/contributing.md`, find the **Making Code Changes** section. Add one sentence before that heading's first paragraph:

```markdown
Rust doc comments and utoipa annotations follow the [Rust doc style](/vault/developer/rustdoc-style/) guide.
```

- [ ] **Step 4: Verify the docs build**

```bash
cd /home/mbeisser/repo/message-vault/docs && npm run check && npm run build
```

Expected: both succeed. `docs/dist/vault/developer/rustdoc-style/index.html` exists after the build.

- [ ] **Step 5: Verify the example rule**

```bash
cd /home/mbeisser/repo/message-vault
grep -cE '`[^`]*\.rs:[0-9]+`' docs/src/content/docs/vault/developer/rustdoc-style.md
```

Expected: at least twice the number of rules (every rule cites two or more `file:line` examples).

- [ ] **Step 6: Commit**

```bash
git add docs/src/content/docs/vault/developer/rustdoc-style.md docs/astro.config.mjs docs/src/content/docs/vault/developer/contributing.md
git commit -m "docs: add Rust doc style guide"
```

---

### Task 6: Final verification

**Files:** none new

**Interfaces:**
- Consumes: Tasks 4 and 5
- Produces: a clean working tree with only this plan's changes committed

- [ ] **Step 1: Confirm the diff touches only intended files**

```bash
cd /home/mbeisser/repo/message-vault
git log --oneline main..HEAD | head
git status --short
```

Expected: the new commits touch only the file map files (`docs/superpowers/reports/2026-08-23-rust-audit.md`, `docs/src/content/docs/vault/developer/rustdoc-style.md`, `docs/src/content/docs/vault/developer/contributing.md`, `docs/astro.config.mjs`, plus the spec committed earlier). No files under `crates/`, `src-tauri/`, or `tests/` changed. `.tmp/` does not appear (gitignored).

- [ ] **Step 2: Confirm no product code changed**

```bash
git diff --stat main..HEAD -- crates src-tauri tests
```

Expected: empty output.

- [ ] **Step 3: Final docs check**

```bash
cd /home/mbeisser/repo/message-vault/docs && npm run check && npm run build
```

Expected: both succeed.

- [ ] **Step 4: Hand off to the user**

Report: where the findings report lives, headline counts, and that the follow-up projects are proposed inside it. The user reviews the report and picks the follow-up work (that is the next brainstorming cycle, one project per group).

---

## Self-review (plan vs spec)

| Spec requirement | Task |
|------------------|------|
| Read-only; only the two deliverables written | Global Constraints; Tasks 1–6 |
| Scope: workspace minus Slint GUI + src-tauri | Task 1 commands exclude `message-vault-io-gui`; finder bases |
| Nine dimensions with definitions and severity meanings | Workflow script `DIMENSIONS`, `SEVERITIES` |
| Six find agents + six verify agents + two synthesize agents | Workflow script |
| Phase 0 mechanical signals before the workflow | Task 1 |
| Adversarial verification; drop failures | Workflow script verify stage |
| Main-loop re-check: all high duplication + >=20% other highs | Task 3 |
| Report at `docs/superpowers/reports/2026-08-23-rust-audit.md` | Task 4 |
| Style guide page + sidebar + Contributing link | Task 5 |
| Style guide covers first-sentence, `//!` intros, examples, banned phrasing, utoipa summary/description, coverage | Task 5 Step 1 |
| Every style rule backed by >=2 codebase examples | Task 5 Steps 1 and 5 |
| Docs build still passes | Tasks 5–6 |
| No dependency bumps; findings only | Global Constraints |
| Workflow failure → no partial report | Task 2 Step 2 (re-run once; Task 3 Step 3 stop gate) |

**Placeholder scan:** none intentionally left; Task 4 Step 3 greps the report for TODO/TBD/XXX/FIXME.

**Type consistency:** finding schema fields (`category`, `severity`, `crate`, `file_line`, `title`, `evidence`, `suggestion`, `group`) are identical in the finder schema, the verify schema (wrapped `finding`), and Tasks 3–4. `style_rules` examples use `file_line`/`quote`/`note` everywhere they are consumed (Tasks 4–5).

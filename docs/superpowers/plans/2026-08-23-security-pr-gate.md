# PR Gate Hardening (SECURITY.md, Dependabot, cargo-deny, npm audit) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give the repository a vulnerability-disclosure path and dependency-scanning gates on every PR, closing issue #105.

**Architecture:** Four additive pieces, all config/docs only — no product code changes. A `SECURITY.md` at the repo root provides the disclosure channel. `.github/dependabot.yml` opens weekly update PRs for the Rust workspace, the Tauri app, the web frontend, the docs site, and CI actions. A `cargo-deny` advisories job and an `npm audit` job join the always-run jobs in `ci.yml`; `scripts/check-pr.sh` mirrors both so the local pre-PR check stays at least as strict as CI. The one substantive fix is clearing the existing `nanoid` advisory (GHSA-2v37-7h3g-55p8) from `web/` and `docs/` so the new audit gate passes at HEAD.

**Tech Stack:** GitHub Actions (ci.yml), Dependabot, cargo-deny (advisories), npm audit, bash (check-pr.sh).

**Spec:** https://github.com/bitrealm-io/message-vault/issues/105 (follow-up to #101 checklist item 4).

## Global Constraints

- CI additions are new jobs in `.github/workflows/ci.yml`, following the existing style: comment banner, `runs-on: ubuntu-latest`, `timeout-minutes: 10`, `actions/checkout@v7` first step. Floating major action tags are fine — Dependabot actions updates will bump them (SHA pinning is out of scope per #105).
- cargo-deny scope is `advisories` only. `licenses` + `bans` stay deferred: `imessage-ir-exporter` depends on GPL-3.0-or-later `imessage-database`, which is not GPL-compatible with the Fair Core License (open caveat from PR #103).
- Do not touch `web-next/` (legacy Next.js UI, not in CI, slated for archive per the #101 assessment).
- npm audit gates at `--audit-level=high` (fails only on high/critical, not the moderate/low noise).
- Commit style matches the repo: `type(scope): summary (#105)`.
- Every docs file must read human-written, not clipped-bullet (docs voice rule).

---

### Task 1: SECURITY.md

**Files:**
- Create: `SECURITY.md`

**Interfaces:**
- Consumes: nothing.
- Produces: the disclosure address `vault@bitrealm.io` and response windows referenced by the PR description.

- [ ] **Step 1: Write the file**

Create `SECURITY.md` with exactly this content:

````markdown
# Security policy

Message Vault stores people's private message archives, so security reports
matter more here than in most projects. If you have found a vulnerability —
or think you may have — we want to hear about it quickly and quietly.

## Supported versions

Message Vault is a self-hosted product: the desktop app and the vault server
both run on machines you control. Because fixes are only ever published for
the current release, security updates apply to the **latest release only**.
If you are running an older version and suspect you are affected by a known
issue, upgrade first and re-test before reporting.

## Reporting a vulnerability

Please do not open a public issue, and please do not post details in
discussions or chat. Report suspected vulnerabilities privately to:

**[vault@bitrealm.io](mailto:vault@bitrealm.io)**

Include as much of the following as you can:

- the affected component and version (server, desktop app, web frontend, or
  a specific exporter),
- a description of the behavior you observed and why it is a problem,
- steps to reproduce, ideally minimal,
- any workaround you have found.

Reports are read by the maintainer. There is no bug bounty program, and
there is no PGP key for this address yet — both may change as the project
grows.

## What to expect

- **Acknowledgment within 5 business days**, with an initial assessment.
- **Updates at least every 14 days** while the report is open.
- **A fix, a scheduled fix, or a written explanation within 90 days** of
  acknowledgment for confirmed vulnerabilities. Most fixes land far sooner;
  the window exists for reports that need a coordinated disclosure or a
  careful migration.
- **Credit in the changelog** for reporters who want it. If you prefer to
  stay anonymous, that is fine too.

## Disclosure

By default we follow coordinated disclosure: the report stays private until
a fix is released, and we agree on a disclosure date with the reporter.
Once the fix ships, reporters are encouraged to publish their write-up —
the changelog entry for the fix will credit them if they wish.
````

- [ ] **Step 2: Self-check**

Read the file once as a stranger would: it must not promise anything the project does not do (no PGP key claimed, no bounty claimed) and must read as prose, not a checklist dump.

- [ ] **Step 3: Commit**

```bash
git add SECURITY.md
git commit -m "docs(security): add SECURITY.md with disclosure policy (#105)"
```

---

### Task 2: .github/dependabot.yml

**Files:**
- Create: `.github/dependabot.yml`

**Interfaces:**
- Consumes: the workspace layout from `Cargo.toml` (workspace members in `crates/`, `src-tauri` excluded with its own `Cargo.lock`), `web/package-lock.json`, `docs/package-lock.json`.
- Produces: nothing for later tasks; GitHub reads this file only from the default branch after merge.

- [ ] **Step 1: Write the file**

Create `.github/dependabot.yml` with exactly this content:

```yaml
version: 2
updates:
  # Rust workspace (all crates in Cargo.toml members)
  - package-ecosystem: cargo
    directory: /
    schedule:
      interval: weekly

  # Tauri desktop app — excluded from the workspace, has its own Cargo.lock
  - package-ecosystem: cargo
    directory: /src-tauri
    schedule:
      interval: weekly

  # Product web frontend
  - package-ecosystem: npm
    directory: /web
    schedule:
      interval: weekly

  # Documentation site
  - package-ecosystem: npm
    directory: /docs
    schedule:
      interval: weekly

  # web-next/ is intentionally excluded: legacy Next.js browse UI, not in CI
  # and not served (see docs/superpowers/reports/2026-08-23-full-stack-assessment.md).
  # Re-add it here if that tree is ever revived.

  # CI workflows
  - package-ecosystem: github-actions
    directory: /
    schedule:
      interval: weekly
```

- [ ] **Step 2: Sanity-check the paths**

Run: `ls Cargo.lock src-tauri/Cargo.lock web/package-lock.json docs/package-lock.json`
Expected: all four files exist (Dependabot needs the lockfiles; each `directory` must contain its manifest).

- [ ] **Step 3: Validate YAML**

Run: `python3 -c "import yaml; yaml.safe_load(open('.github/dependabot.yml'))"`
Expected: no output, exit 0.

- [ ] **Step 4: Commit**

```bash
git add .github/dependabot.yml
git commit -m "ci(deps): enable Dependabot for cargo, npm, and GitHub Actions (#105)"
```

Note: Dependabot reads this file only once merged to the default branch — it cannot be exercised from a PR.

---

### Task 3: cargo-deny advisories gate

**Files:**
- Create: `deny.toml`
- Modify: `.github/workflows/ci.yml` (insert new job after the `test` job)
- Modify: `scripts/check-pr.sh` (insert after the license-consistency step)

**Interfaces:**
- Consumes: the workspace lockfile `Cargo.lock`.
- Produces: the `deny` CI job name; later tasks do not depend on it.

- [ ] **Step 1: Write deny.toml**

Create `deny.toml` with exactly this content (written for cargo-deny >= 0.18, where vulnerability advisories always error and `ignore` is the only silencing mechanism):

```toml
# cargo-deny configuration. CI runs `cargo deny check advisories` on every PR
# (see .github/workflows/ci.yml, job `deny`).
#
# Only advisories are checked today. `licenses` and `bans` are deliberately
# deferred: imessage-ir-exporter depends on GPL-3.0-or-later
# imessage-database, which is not GPL-compatible with the Fair Core License.
# That coupling needs a real resolution before license/bans checks can gate
# the build (open caveat from PR #103).

[advisories]
# In cargo-deny >= 0.18 vulnerability advisories always fail the check and
# unmaintained/unsound are scopes (default: check all), so specific findings
# can only be silenced with `ignore`. Each entry below is deliberate; a new
# advisory still fails the gate until it is fixed or ignored here.

ignore = [
    # Unmaintained crates, transitive and unfixable today:
    "RUSTSEC-2025-0052", # async-std discontinued (dev-dep via httpmock)
    "RUSTSEC-2024-0436", # paste unmaintained (via image/exr)
    "RUSTSEC-2026-0206", # rustybuzz unmaintained (legacy Slint GUI's usvg)
    "RUSTSEC-2026-0192", # ttf-parser unmaintained (legacy Slint GUI's usvg)
    { id = "RUSTSEC-2026-0194", reason = "quick-xml quadratic duplicate-attribute scan: pinned by imessage-database =4.2.0 -> plist =1.9.0 -> quick-xml 0.39.4; parses the user's own local macOS/iOS data, never network input. Revisit when imessage-database releases." },
    { id = "RUSTSEC-2026-0195", reason = "quick-xml unbounded namespace allocation: same chain and exposure as RUSTSEC-2026-0194." },
]
```

- [ ] **Step 2: Add the CI job**

In `.github/workflows/ci.yml`, insert after the `test` job (after its `cargo test --workspace` step, before the `# ── Always: web Biome (lint + format) ──` banner):

```yaml
  # ── Always: cargo-deny advisories ─────────────────────────────────────
  deny:
    name: cargo-deny (advisories)
    runs-on: ubuntu-latest
    timeout-minutes: 10
    steps:
      - uses: actions/checkout@v7

      - uses: EmbarkStudios/cargo-deny-action@v2
        with:
          command: check advisories
```

- [ ] **Step 3: Validate the workflow YAML**

Run: `python3 -c "import yaml; yaml.safe_load(open('.github/workflows/ci.yml'))"`
Expected: no output, exit 0. (YAML parse only — GitHub validates the full syntax at push time.)

- [ ] **Step 4: Mirror in check-pr.sh**

In `scripts/check-pr.sh`, insert after the license-consistency block (after line `"${SCRIPT_DIR}/check-license.sh"`) and before the `cargo build --workspace` block:

```bash
echo "==> cargo deny check advisories"
if cargo deny --version >/dev/null 2>&1; then
  cargo deny check advisories
else
  echo "cargo-deny not installed; skipping advisory check (CI enforces it)" >&2
fi
```

- [ ] **Step 5: Install cargo-deny locally**

Run: `cargo install cargo-deny --locked`
Expected: compiles for a few minutes, then exits 0. (`cargo deny --version` prints a version afterward.)

- [ ] **Step 6: Run the check locally**

Run: `cargo deny check advisories`
Expected: exit 0 and `advisories ok` (the first run downloads the advisory database).

Contingency — only if the check denies something:
- If a fixed version of the offending crate exists: run `cargo update -p <crate>`, re-run `cargo deny check advisories` and `cargo test --workspace`, and include the `Cargo.lock` change in the commit.
- If no fixed version exists: add an `ignore` entry for the advisory id under `[advisories]` with a comment citing the advisory URL, and note it in the PR description so it is reviewed, not silently swallowed.

**What happened at execution time** (kept for the record): cargo-deny 0.20.2 reported 2 vulnerabilities and 4 unmaintained advisories at HEAD. Both vulnerabilities are in quick-xml 0.39.4, which is pinned by `imessage-database =4.2.0 -> plist =1.9.0` (verified: `imessage-database` has no newer release, `plist` 1.10.0 is blocked by the `=1.9.0` pin, and quick-xml >= 0.41.0 cannot satisfy plist 1.9.0). The affected parser only reads the user's own local macOS/iOS data, so both were recorded as `ignore` entries with reasons (above). The four unmaintained advisories are transitive and unfixable (dev-deps, image's dep tree, legacy Slint GUI), likewise ignored with reasons. `unmaintained` scope was left at the default `all` so any *new* unmaintained advisory still fails the gate and gets reviewed rather than silently passing.

- [ ] **Step 7: Commit**

```bash
git add deny.toml .github/workflows/ci.yml scripts/check-pr.sh
git commit -m "ci(security): gate PRs on cargo-deny advisories (#105)"
```

---

### Task 4: Clear the nanoid advisory from web/ and docs/

**Files:**
- Modify: `web/package-lock.json`, `docs/package-lock.json` (via `npm audit fix` — verified at plan time: no source change is needed, the fix is in-range transitive bump)

**Interfaces:**
- Consumes: nothing.
- Produces: clean `npm audit` output in both workspaces, which Task 5's gate depends on. Tests must stay green for the same lockfiles Task 5's `npm ci` installs.

- [ ] **Step 1: Apply the fix in web/**

Run:
```bash
cd web
npm audit fix
npm audit
```
Expected: `npm audit fix` changes `package-lock.json` only; the second command ends with `found 0 vulnerabilities`.

- [ ] **Step 2: Verify web still passes**

Run:
```bash
cd web
npm ci
npm test
npm run lint
```
Expected: all Vitest tests pass and `biome ci`-equivalent lint passes (same as the `web-lint`/`web-test` CI jobs).

- [ ] **Step 3: Apply the fix in docs/**

Run:
```bash
cd docs
npm audit fix
npm audit
```
Expected: `found 0 vulnerabilities`.

- [ ] **Step 4: Verify docs still build**

Run:
```bash
cd docs
npm ci
npm run check
npm run build
```
Expected: both succeed (same as `scripts/check-pr.sh`'s docs steps).

- [ ] **Step 5: Commit**

```bash
git add web/package-lock.json docs/package-lock.json
git commit -m "fix(deps): resolve nanoid advisory GHSA-2v37-7h3g-55p8 in web and docs (#105)"
```

---

### Task 5: npm audit gate in CI and check-pr.sh

**Files:**
- Modify: `.github/workflows/ci.yml` (insert new job after the `web-test` job)
- Modify: `scripts/check-pr.sh` (web audit after web test; docs audit after docs build)

**Interfaces:**
- Consumes: the clean lockfiles from Task 4 (the job's `npm ci` + `npm audit` must pass).
- Produces: the `npm-audit` CI job name.

- [ ] **Step 1: Add the CI job**

In `.github/workflows/ci.yml`, insert after the `web-test` job (after its `npm test` step, before the `# ── Always: license consistency ──` banner):

```yaml
  # ── Always: npm audit (web + docs) ────────────────────────────────────
  npm-audit:
    name: Audit npm deps
    runs-on: ubuntu-latest
    timeout-minutes: 10
    steps:
      - uses: actions/checkout@v7

      - name: Install Node.js
        uses: actions/setup-node@v7
        with:
          node-version: 22
          cache: npm
          cache-dependency-path: |
            web/package-lock.json
            docs/package-lock.json

      - name: Audit web frontend
        run: |
          cd web
          npm ci
          npm audit --audit-level=high

      - name: Audit docs site
        run: |
          cd docs
          npm ci
          npm audit --audit-level=high
```

- [ ] **Step 2: Validate the workflow YAML**

Run: `python3 -c "import yaml; yaml.safe_load(open('.github/workflows/ci.yml'))"`
Expected: no output, exit 0.

- [ ] **Step 3: Mirror in check-pr.sh**

In `scripts/check-pr.sh`, insert after the web test block (after `(cd web && npm test)`):

```bash
echo "==> web audit"
(cd web && npm audit --audit-level=high)
```

And insert after the docs build block (after `(cd docs && npm run build)`):

```bash
echo "==> docs audit"
(cd docs && npm audit --audit-level=high)
```

- [ ] **Step 4: Run the full local check-pr script once**

Run: `./scripts/check-pr.sh`
Expected: completes end-to-end (this also re-verifies Tasks 3–5 interact correctly). If `cargo-deny` is installed from Task 3, the advisory check runs here too.

- [ ] **Step 5: Commit**

```bash
git add .github/workflows/ci.yml scripts/check-pr.sh
git commit -m "ci(security): gate PRs on npm audit for web and docs (#105)"
```

---

### Task 6: Push, PR, and issue bookkeeping

**Files:**
- None new (the plan document itself is committed here).

**Interfaces:**
- Consumes: all prior tasks' commits.

- [ ] **Step 1: Commit the plan document**

```bash
git add docs/superpowers/plans/2026-08-23-security-pr-gate.md
git commit -m "docs: add security PR-gate implementation plan (#105)"
```

- [ ] **Step 2: Review the full diff before pushing**

Run: `git log --oneline origin/main..HEAD` and `git diff origin/main..HEAD --stat`
Expected: 6 commits, touching exactly `SECURITY.md`, `.github/dependabot.yml`, `deny.toml`, `.github/workflows/ci.yml`, `scripts/check-pr.sh`, `web/package-lock.json`, `docs/package-lock.json`, and the plan doc. Nothing under `web-next/`, no product code.

- [ ] **Step 3: Push and open the PR**

```bash
git push -u origin worktree-issue-105-security-pr-gate
gh pr create \
  --title "chore(ci): SECURITY.md, Dependabot, cargo-deny, and npm audit (#105)" \
  --body-file - <<'EOF'
Closes #105.

Adds the four pieces from the issue:

1. **SECURITY.md** — disclosure via vault@bitrealm.io, 5-business-day acknowledgment, 14-day update cadence, 90-day fix window, coordinated disclosure.
2. **.github/dependabot.yml** — weekly updates for the Rust workspace, `src-tauri`, `web/`, `docs/`, and GitHub Actions. `web-next/` is intentionally excluded (legacy, not in CI, slated for archive).
3. **cargo-deny advisories** — new `deny` job on every PR (`cargo deny check advisories`), mirrored in `check-pr.sh` when the tool is installed. `licenses`/`bans` stay deferred until the GPL `imessage-database` coupling is resolved (PR #103 caveat).
4. **npm audit** — new `npm-audit` job gating `web/` and `docs/` at `--audit-level=high`, mirrored in `check-pr.sh`.

Also fixes the existing nanoid advisory (GHSA-2v37-7h3g-55p8) in `web/` and `docs/` lockfiles so the new gate passes at HEAD; web tests and the docs build verified after the bump.

**Verified locally:** `cargo deny check advisories` passes; `npm audit` reports 0 vulnerabilities in both workspaces; `web` Vitest + lint pass; docs check + build pass; both YAML files parse.
EOF
```

- [ ] **Step 4: Tick the issue checkboxes (optional but nice)**

The four task lines in issue #105 are `- [ ] Add \`SECURITY.md\`…`, `- [ ] Add \`dependabot.yml\`…`, `- [ ] Add \`cargo-deny\`…`, `- [ ] Consider \`npm audit\`…`. If convenient after the PR is open, flip each `- [ ]` to `- [x]` via `gh api -X PATCH repos/bitrealm-io/message-vault/issues/105 -f body="$(...)"` — only if the full body can be round-tripped unchanged otherwise.

- [ ] **Step 5: Report**

Summarize: PR URL, what was verified, the `web-next` exclusion, the deferred licenses/bans, and the two repo settings Dependabot needs to be useful (vulnerability alerts + automated security fixes under Settings → Code security — not toggled here unless the API permits, in which case say so).

---

## Self-review notes

- Spec coverage: all four issue checkboxes map to Tasks 1, 2, 3, and 4+5; out-of-scope items (CodeQL, SHA pinning) are untouched.
- The nanoid advisory fix (Task 4) is included because the gate from Task 5 fails at HEAD without it — verified during research (`npm audit` reports 1 high in web/, 2 high in docs/).
- Placeholder scan: every task contains the exact file content or exact command; no TBD/TODO/"similar to Task N" anywhere.
- Consistency: the CI job names `deny` and `npm-audit`, the commit subjects, and the paths all match between tasks and the PR body.

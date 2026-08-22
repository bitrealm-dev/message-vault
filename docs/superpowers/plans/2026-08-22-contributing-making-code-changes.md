# Contributing Making Code Changes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fill **Making Code Changes** on the contributing page with the issue-first branch-and-commit flow, and remove the leftover **Before You Code** and **Making Changes** stubs.

**Architecture:** Edit `docs/src/content/docs/vault/developer/contributing.md` in place. Replace the empty heading plus two stubs with the approved copy. Leave later placeholder sections and the older material below the `---------------------------------` separator untouched.

**Tech Stack:** Astro Starlight Markdown under `docs/src/content/docs/`.

**Spec:** `docs/superpowers/specs/2026-08-22-contributing-making-code-changes-design.md`

## Global Constraints

- Voice: short sentences, concrete commands, no “we” / “us” / “our”.
- Do not use GitHub `> [!TIP]` / `> [!WARNING]` alerts.
- Do not teach opening a pull request in this section.
- Do not add DCO / `--signoff`, rebase/fixup recipes, or “wait for a go-ahead”.
- Do not rewrite **Code Standards**, **Pull Request Checklist**, **Response Time**, **First-Time Contributors**, or **Questions**.
- Do not move the `---------------------------------` separator or the older material below it.
- Do not change `.github/` issue or pull request templates.
- Do not change runtime, exporter, desktop app, or vault-server code.

---

## File map

| File | Role |
|------|------|
| `docs/src/content/docs/vault/developer/contributing.md` | Contributor guide. Fill **Making Code Changes**; delete **Before You Code** and **Making Changes**. |
| `docs/superpowers/specs/2026-08-22-contributing-making-code-changes-design.md` | Approved copy source. Do not edit during implementation. |
| `.github/ISSUE_TEMPLATE/bug_report.md` | Do not edit. Already linked from **Reporting Bugs or Requesting Features**. |
| `.github/ISSUE_TEMPLATE/feature_request.md` | Do not edit. Already linked from **Reporting Bugs or Requesting Features**. |

---

### Task 1: Replace the stubs with Making Code Changes

**Files:**
- Modify: `docs/src/content/docs/vault/developer/contributing.md` (from `## Making Code Changes` through the end of `## Making Changes`, immediately before `## Code Standards`)

**Interfaces:**
- Consumes: existing heading `## Making Code Changes` after **Build and Run**; approved copy in the spec
- Produces: one **Making Code Changes** section; next heading remains `## Code Standards`

- [ ] **Step 1: Replace the empty heading and two stubs**

In `docs/src/content/docs/vault/developer/contributing.md`, find this exact block (blank lines included):

```markdown
## Making Code Changes



## Before You Code

For anything beyond typos:
- Open an issue first and describe your idea
- Wait for a maintainer to signal it's welcome
- This saves your time and ours!

## Making Changes

Branch naming:
- feat/description - new features
- fix/description - bug fixes
- docs/description - documentation

Commit messages (Conventional Commits):
- feat: add user authentication
- fix: handle null values in parser
- docs: improve setup instructions

## Code Standards
```

Replace it with this exact block (keep `## Code Standards` as the following heading):

````markdown
## Making Code Changes

Open a GitHub issue before starting the work, so the later pull request can link to it. Use the bug report or feature request form. Do not wait for a reply before coding. If the issue has no reply after 5 business days, comment on that same issue.

**Branch**

Start from the latest `main`. Do not commit on `main`. Name the branch with a prefix:

- `feat/short-name` — new behavior
- `fix/short-name` — a bug
- `docs/short-name` — documentation only

Keep the branch current with `main` while working (merge or rebase). One pull request should do one job.

**Commits**

Each commit should be one idea. Do not mix a bug fix with a rename, or a feature with formatting of unrelated files.

Prefer `feat:`, `fix:`, or `docs:` at the start of the subject when it fits. Other prefixes are optional. The subject should say what changed. Add a short body when the reason is not obvious. Mention the issue (`Ref: #123`).

**Example**

After the fork is cloned, from the repository root:

```bash
git remote add upstream https://github.com/bitrealm-io/message-vault.git
git fetch upstream
git checkout -b feat/short-name upstream/main
git commit -m "feat: add support for x

Ref: #123"
git push -u origin feat/short-name
```

Add `upstream` once. For later branches: `git fetch upstream`, then `git checkout -b … upstream/main`.

## Code Standards
````

Do not edit **Build and Run** in this task. Do not edit anything after `## Code Standards`.

- [ ] **Step 2: Confirm the stubs are gone and later placeholders remain**

Search the same file:

```bash
rg -n "Before You Code|Making Changes|wait for a maintainer|Signed-off-by|git commit --signoff" docs/src/content/docs/vault/developer/contributing.md
```

Expected:

- No `## Before You Code`
- No `## Making Changes`
- No “wait for a maintainer”
- No DCO / `--signoff`
- `## Code Standards` still present
- `## Pull Request Checklist` still present
- The `---------------------------------` separator still present

- [ ] **Step 3: Confirm voice and placement**

Read the new section. Check:

- It sits immediately after **Stopping and restarting** / **Build and Run**
- No “we”, “us”, or “our” in the new section
- No `> [!TIP]` or `> [!WARNING]`
- The git example uses `upstream` `https://github.com/bitrealm-io/message-vault.git` and `Ref: #123`

- [ ] **Step 4: Check and build the docs site**

```bash
cd docs && npm run check && npm run build
```

Expected: exit 0.

```bash
rg -n "Making Code Changes|feat/short-name|5 business days" docs/dist/vault/developer/contributing/index.html
```

Expected: those strings appear in the built page.

- [ ] **Step 5: Commit only this file**

If the working tree has other contributing-page edits (for example **Build and Run**), do not include them unless they are already on the branch and intended. For this task, stage the contributing page after the replacement above, plus this plan only if it is still untracked.

```bash
git add docs/src/content/docs/vault/developer/contributing.md
git commit -m "docs: add Making Code Changes to contributing guide

Tell contributors to file an issue, branch from main with feat/fix/docs, and keep commits to one idea."
```

If `git status` still shows unrelated files (`.github/CONTRIBUTING.md`, root `CONTRIBUTING.md`, `README.md`), leave them unstaged.

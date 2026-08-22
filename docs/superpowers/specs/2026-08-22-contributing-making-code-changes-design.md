# Contributing: Making Code Changes

**Date:** 2026-08-22  
**Status:** Approved for planning

## Context

`docs/src/content/docs/vault/developer/contributing.md` is being rewritten as a first-run contributor guide. **Environment Setup** and **Build and Run** already exist. The next heading, **Making Code Changes**, is empty. Below it sit leftover template stubs (**Before You Code**, **Making Changes**, PHP/Pest **Code Standards**, and later placeholder sections).

The project has one maintainer. GitHub issue forms and a pull request template already exist. There is no DCO sign-off, commit lint, or GitHub Discussions contact in the repo.

The [tenthirtyam contributor-flow article](https://tenthirtyam.org/dispatches/2026/03/21/writing-practical-contribution-guidelines-for-github-repositories/#contributor-flow) is the structural reference. This section is a shorter version of that flow: issue, branch, commits, one git example. It is not a full pull-request handbook.

## Goal

A new contributor who has the vault running can: open an issue, branch from `main`, make focused commits, and push. The page states those rules in one short section that can be read in a couple of minutes.

## Non-goals

- Opening, drafting, or updating a pull request (later section)
- Local test commands (already drafted later as **Test before a pull request**)
- Rewriting **Code Standards**, **Pull Request Checklist**, **Response Time**, **First-Time Contributors**, or **Questions** in this change (those stubs stay until their own pass)
- DCO / `git commit --signoff`
- Rebase, `--fixup`, or `--force-with-lease` recipes
- Requiring a maintainer go-ahead before coding
- Conventional Commits as a hard rule (scopes, `chore`, `BREAKING CHANGE`, commit lint)
- Changing `.github/` issue or pull request templates
- Runtime, exporter, desktop app, or vault-server code

## Decisions

1. **Issue first, then start.** Open a GitHub issue before starting the work, using the bug report or feature request form. Do not wait for a reply. The later pull request links to that issue.
2. **Ping after 5 business days.** If the issue has no reply after 5 business days, comment on that same issue. No email and no Discussions link.
3. **Branch from latest `main`.** Do not commit on `main`. Prefixes: `feat/`, `fix/`, `docs/`. Keep the branch current with `main` (merge or rebase). One pull request does one job.
4. **Logical commits, light prefixes.** Each commit is one idea. Prefer `feat:`, `fix:`, or `docs:` on the subject when it fits. Other prefixes are optional. Subject says what changed. Short body when the reason is not obvious. Mention the issue (`Ref: #123`).
5. **Numbered flow plus one git example.** Teach `upstream` once, then branch, commit, push. Leave pull-request opening for a later section.

## What changes

Edit `docs/src/content/docs/vault/developer/contributing.md` in place.

- Fill **Making Code Changes** with the copy below.
- Delete the **Before You Code** and **Making Changes** stubs. This section replaces them.
- Do not move the `---------------------------------` separator or the older material below it in this change.

## Intended copy

### Making Code Changes

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

## Voice

Match the rest of the new contributing page: short sentences, concrete commands, no “we” / “us” / “our”. Starlight asides are optional; do not use GitHub `> [!TIP]` alerts (they do not render the same in the docs site).

## Verification

- The section sits immediately after **Build and Run**
- **Before You Code** and **Making Changes** headings are gone
- Copy matches the intended text (issue first, 5-business-day comment on the same issue, branch prefixes, light commit prefixes, one git example)
- No DCO, rebase cookbook, or “wait for a go-ahead”
- `cd docs && npm run check && npm run build` still succeeds

## Success criteria

- A first-time contributor knows to file an issue, branch from `main` with `feat/` `fix/` or `docs/`, keep that branch current, and make one-idea commits
- The page does not teach opening a pull request in this section

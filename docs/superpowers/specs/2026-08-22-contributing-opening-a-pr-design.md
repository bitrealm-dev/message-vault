# Contributing: Opening a PR

**Date:** 2026-08-22  
**Status:** Approved for planning

## Context

`docs/src/content/docs/vault/developer/contributing.md` now has **Making Code Changes** (issue, branch, commits, push) and a **Test before a pull request** subsection. The next missing heading is how to open a pull request.

The project has one maintainer. GitHub already has:

- Default form: `.github/pull_request_template.md`
- Optional feature form: `.github/PULL_REQUEST_TEMPLATE/feature.md`
- Optional bug-fix form: `.github/PULL_REQUEST_TEMPLATE/bugfix.md`

There is no DCO sign-off, commit lint, or draft-PR requirement.

The [tenthirtyam contributor-flow article](https://tenthirtyam.org/dispatches/2026/03/21/writing-practical-contribution-guidelines-for-github-repositories/#contributor-flow) is still the structural reference. This section is the short “when opening” part: against `main`, fill a template, link the issue, stay current with `main`. It is not a rebase or review-reply handbook.

## Goal

A contributor who has already pushed a topic branch can open a pull request against `main`, fill a template, link the issue, and update the branch if `main` has moved.

## Non-goals

- Rewriting **Making Code Changes**, **Test before a pull request**, **Docs site**, **Workspace map**, **License**, or **Contribution rules**
- Changing `.github/` pull request templates
- Requiring a feature or bug-fix template
- Requiring GitHub drafts
- DCO / `--signoff`
- `--fixup` or `--force-with-lease` recipes
- Conventional Commits as a hard rule for titles
- Runtime, exporter, desktop app, or vault-server code

## Decisions

1. **Open against `main`.** The pull request targets `bitrealm-io/message-vault` `main`.
2. **Default template is enough.** GitHub fills in the default form. Feature and bug-fix templates exist and may be used; they are not required.
3. **Link the issue.** Use `Ref: #123`. Use `Fixes #123` in the description if this change should close that issue.
4. **Light title prefixes.** Prefer `feat:`, `fix:`, or `docs:` at the start of the title when it fits. Other prefixes are optional.
5. **Stay current with merge.** Show `git fetch upstream`, `git merge upstream/main`, `git push`. Rebase is allowed. Merge is enough. Do not teach `--force` except a one-line warning: do not force-push unless the branch is only used by that contributor.
6. **Optional `gh`.** One `gh pr create` example. The GitHub website is enough.
7. **After open.** GitHub runs checks. Fix failing checks. Reply to review comments on the same pull request.

## What changes

Edit `docs/src/content/docs/vault/developer/contributing.md` in place.

Insert **Opening a PR** after **Test before a pull request** (end of **Making Code Changes**) and before **Docs site**.

Do not edit `.github/pull_request_template.md` or files under `.github/PULL_REQUEST_TEMPLATE/`.

## Intended copy

### Opening a PR

A pull request asks to merge the branch into `main`. Open it against `main` on [bitrealm-io/message-vault](https://github.com/bitrealm-io/message-vault). Use the GitHub pull request form. GitHub fills in the default template. That default is enough for most changes. Feature and bug-fix templates also exist; they are not required.

Link the issue (`Ref: #123`). Write `Fixes #123` in the description if this change should close that issue.

Prefer `feat:`, `fix:`, or `docs:` at the start of the title when it fits.

From the repository root, this also works:

```bash
gh pr create --base main --title "feat: add support for x" --body "Ref: #123"
```

**Keep the branch current**

If `main` has moved, update the branch before asking for review:

```bash
git fetch upstream
git merge upstream/main
git push
```

Rebase is allowed. Merge is enough. Do not force-push unless the branch is only used by that one contributor.

**After it is open**

GitHub runs checks. Fix failing checks. Reply to review comments on the same pull request.

## Voice

Match the rest of the new contributing page: short sentences, concrete commands, no “we” / “us” / “our”. Do not use GitHub `> [!TIP]` alerts.

## Verification

- **Opening a PR** sits after **Test before a pull request** and before **Docs site**
- Copy mentions the default template is enough and that feature/bug-fix templates are optional
- Copy includes `Ref: #123` / `Fixes #123`, merge-to-stay-current, optional `gh pr create`
- No DCO, draft-PR requirement, or `--force-with-lease` cookbook
- `.github/` templates unchanged
- `cd docs && npm run check && npm run build` still succeeds

## Success criteria

- A first-time contributor can open a PR against `main`, fill the default template, link an issue, and update the branch if `main` moved
- Feature and bug-fix templates are named as optional, not required

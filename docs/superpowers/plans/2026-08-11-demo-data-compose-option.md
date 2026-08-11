# DEMO_DATA Compose Option Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace Docker `VAULT_MODE=demo|personal` with a single vault runtime and optional first-boot seeding via `DEMO_DATA` (default `true`).

**Architecture:** Compose passes `DEMO_DATA`; both entrypoints seed with `reset-demo` + `process-assets` only when `data/vault.db` is missing and `DEMO_DATA` is truthy. Docs and comments stop calling this a vault “mode.” The server binary is unchanged.

**Tech Stack:** Bash entrypoints, Docker Compose YAML, Markdown docs (Starlight + README/CLAUDE).

**Spec:** [docs/superpowers/specs/2026-08-11-demo-data-compose-option-design.md](../specs/2026-08-11-demo-data-compose-option-design.md)

## Global Constraints

- Default `DEMO_DATA=true` in compose (`${DEMO_DATA:-true}`).
- Truthy seed values (case-insensitive): `true`, `1`, `yes`. Everything else → no seed.
- If `data/vault.db` exists, never seed (regardless of `DEMO_DATA`).
- Remove `VAULT_MODE` from compose, entrypoints, and live user/docs snippets listed in the spec (no compat alias).
- Do not change `reset-demo` CLI behavior or write `demo_data` into `config.toml`.
- Do not rewrite historical archives under `docs/superpowers/plans/` except this new plan file.

## File map

| File | Role |
|------|------|
| `compose-dev.yml` | Pass `DEMO_DATA`; drop `VAULT_MODE` |
| `compose-release.yml` | Same |
| `scripts/docker-entrypoint-dev.sh` | Seed on empty DB when `DEMO_DATA` truthy |
| `scripts/docker-entrypoint-release.sh` | Same (uses `message-vault-server` binary) |
| `README.md`, `CLAUDE.md` | Quick-start env examples |
| `docs/src/content/docs/**` (listed below) | User-facing Docker instructions |

---

### Task 1: Entrypoints use `DEMO_DATA`

**Files:**
- Modify: `scripts/docker-entrypoint-dev.sh`
- Modify: `scripts/docker-entrypoint-release.sh`

**Interfaces:**
- Consumes: env `DEMO_DATA` (default `true`); presence of `data/vault.db`
- Produces: same side effects as today — either seeded demo DB or empty `data/` then `serve`

- [ ] **Step 1: Replace `VAULT_MODE` logic in `scripts/docker-entrypoint-release.sh`**

Use this shape (keep existing `ensure_docker_config`, paths, and `message-vault-server` commands):

```bash
DEMO_DATA="${DEMO_DATA:-true}"

demo_data_requested() {
  case "$(printf '%s' "${DEMO_DATA}" | tr '[:upper:]' '[:lower:]')" in
    true|1|yes) return 0 ;;
    *) return 1 ;;
  esac
}

seed_if_needed() {
  if [[ -f data/vault.db ]]; then
    echo "Vault DB present; skipping seed (DEMO_DATA=${DEMO_DATA})."
    ensure_docker_config
    return
  fi

  ensure_docker_config

  if demo_data_requested; then
    echo "Seeding demo data (DEMO_DATA=${DEMO_DATA})…"
    message-vault-server reset-demo --config "${CONFIG}"
    ensure_docker_config
    echo "Converting demo media…"
    message-vault-server process-assets --config "${CONFIG}" \
      || echo "warning: process-assets failed; UI still works"
  else
    echo "DEMO_DATA=${DEMO_DATA}: empty data/ (create an account in the web UI)."
  fi
}
```

Remove the `VAULT_MODE` variable and `case` that required `demo`/`personal`.

- [ ] **Step 2: Mirror the same logic in `scripts/docker-entrypoint-dev.sh`**

Same `demo_data_requested` / `seed_if_needed` structure, but keep the **dev** commands:

```bash
cargo run --release -p message-vault-server -- reset-demo --config "${CONFIG}"
# …
cargo run --release -p message-vault-server -- process-assets --config "${CONFIG}" \
  || echo "warning: process-assets failed; UI still works"
```

Keep the existing `ensure_docker_config` that requires bind-mounted `config/config.docker.toml`, and keep the `/app/static` notes block above `seed_if_needed`.

- [ ] **Step 3: Smoke-check truthy parsing locally (no Docker required)**

```bash
# Extract/test the helper by sourcing a tiny snippet, or run:
bash -c '
demo_data_requested() {
  case "$(printf "%s" "${DEMO_DATA}" | tr "[:upper:]" "[:lower:]")" in
    true|1|yes) return 0 ;;
    *) return 1 ;;
  esac
}
for DEMO_DATA in true TRUE 1 yes Yes false 0 "" personal demo; do
  if demo_data_requested; then echo "$DEMO_DATA -> seed"; else echo "$DEMO_DATA -> no-seed"; fi
done
'
```

Expected:

```
true -> seed
TRUE -> seed
1 -> seed
yes -> seed
Yes -> seed
false -> no-seed
0 -> no-seed
 -> no-seed
personal -> no-seed
demo -> no-seed
```

Note: bare string `demo` is **not** truthy (operators must use `DEMO_DATA=true`).

- [ ] **Step 4: Commit**

```bash
git add scripts/docker-entrypoint-dev.sh scripts/docker-entrypoint-release.sh
git commit -m "$(cat <<'EOF'
feat(docker): seed demo from DEMO_DATA, not VAULT_MODE

One vault runtime; optional first-boot seed when DEMO_DATA is truthy
and data/vault.db is missing.
EOF
)"
```

---

### Task 2: Compose files

**Files:**
- Modify: `compose-dev.yml`
- Modify: `compose-release.yml`

**Interfaces:**
- Consumes: host env `DEMO_DATA` (optional)
- Produces: container env `DEMO_DATA` defaulting to `true`

- [ ] **Step 1: Update `compose-release.yml`**

Header comment — replace the personal-mode line with:

```yaml
# Blank vault (no demo seed): DEMO_DATA=false docker compose -f compose-release.yml up --build
```

Environment — replace `VAULT_MODE` with:

```yaml
    environment:
      DEMO_DATA: ${DEMO_DATA:-true}
      VAULT_AUTH: ${VAULT_AUTH:-local}
      AUTH_MODE: ${AUTH_MODE:-local}
      HANKO_API_URL: ${HANKO_API_URL:-}
```

Keep volumes and build block unchanged.

- [ ] **Step 2: Update `compose-dev.yml`**

Header comment:

```yaml
# Blank vault (no demo seed): DEMO_DATA=false docker compose -f compose-dev.yml up
```

Environment:

```yaml
    environment:
      DEMO_DATA: ${DEMO_DATA:-true}
      VAULT_AUTH: ${VAULT_AUTH:-local}
      HANKO_API_URL: ${HANKO_API_URL:-}
```

- [ ] **Step 3: Verify no `VAULT_MODE` left in compose/entrypoints**

```bash
rg 'VAULT_MODE' compose-dev.yml compose-release.yml scripts/docker-entrypoint-dev.sh scripts/docker-entrypoint-release.sh
```

Expected: no matches.

- [ ] **Step 4: Commit**

```bash
git add compose-dev.yml compose-release.yml
git commit -m "$(cat <<'EOF'
chore(docker): pass DEMO_DATA from Compose

Default true; DEMO_DATA=false for an empty first-boot volume.
EOF
)"
```

---

### Task 3: Docs and maintainer snippets

**Files:**
- Modify: `README.md`
- Modify: `CLAUDE.md`
- Modify: `docs/src/content/docs/introduction/quick-start.md`
- Modify: `docs/src/content/docs/set-up-the-server/docker-install.md`
- Modify: `docs/src/content/docs/set-up-the-server/try-the-demo.md`
- Modify: `docs/src/content/docs/set-up-the-server/first-personal-vault.md`
- Modify: `docs/src/content/docs/set-up-the-server/updating.md`

**Interfaces:**
- Consumes: Task 1–2 env contract (`DEMO_DATA=true|false`)
- Produces: docs that never say `VAULT_MODE` or “demo mode / personal mode” as Docker runtimes

- [ ] **Step 1: Replace docker `run` / compose examples**

In each file above, apply these substitutions consistently:

| Old | New |
|-----|-----|
| `-e VAULT_MODE=demo` | omit (default seeds) **or** `-e DEMO_DATA=true` where an explicit flag helps |
| `-e VAULT_MODE=personal` | `-e DEMO_DATA=false` |
| `VAULT_MODE=personal docker compose …` | `DEMO_DATA=false docker compose …` |
| “demo mode” / “personal mode” as the Docker switch | “seed demo data” / “empty vault” / `DEMO_DATA` |

**`try-the-demo.md`:** keep the volume-wipe reset recipe; swap env to `DEMO_DATA=true` (or omit). Mention that an existing volume is not reseeded by flipping `DEMO_DATA`, and that in-place refresh uses CLI `reset-demo` (already referenced from settings docs).

**`first-personal-vault.md`:** title/purpose can stay “personal vault”; start instructions must use `DEMO_DATA=false`, not `VAULT_MODE=personal`.

**`docker-install.md`:** table/rows that say “demo mode only” → “when demo data is seeded” / `DEMO_DATA=true`.

**`CLAUDE.md`:** replace `VAULT_MODE=personal docker compose up` with `DEMO_DATA=false docker compose up`.

**`README.md`:** same pattern as quick-start.

- [ ] **Step 2: Grep live docs for leftover `VAULT_MODE`**

```bash
rg 'VAULT_MODE' README.md CLAUDE.md docs/src/content/docs
```

Expected: no matches.

(Allowed leftovers only under `docs/superpowers/plans/` historical archives — do not edit those.)

- [ ] **Step 3: Commit**

```bash
git add README.md CLAUDE.md docs/src/content/docs
git commit -m "$(cat <<'EOF'
docs: document DEMO_DATA instead of VAULT_MODE

Docker has one vault runtime; demo dataset is an optional first-boot seed.
EOF
)"
```

---

### Task 4: Acceptance verification

**Files:** none (manual / local checks)

- [ ] **Step 1: Confirm entrypoint + compose grep clean**

```bash
rg 'VAULT_MODE' compose-dev.yml compose-release.yml scripts/docker-entrypoint-*.sh README.md CLAUDE.md docs/src/content/docs
```

Expected: no matches.

- [ ] **Step 2: Optional Docker smoke (if Docker available)**

Empty volume + default:

```bash
docker compose -f compose-release.yml down
# remove the project volume (name varies; use docker volume ls)
DEMO_DATA=true docker compose -f compose-release.yml up --build -d
# wait until healthy / logs show "Seeding demo data"
docker compose -f compose-release.yml logs vault | head -80
```

Expected log lines include seeding (and not `VAULT_MODE`).

Blank vault:

```bash
docker compose -f compose-release.yml down
# remove volume again
DEMO_DATA=false docker compose -f compose-release.yml up --build -d
docker compose -f compose-release.yml logs vault | head -80
```

Expected: log mentions empty data / create account; no `reset-demo` seed.

Skip this step if the environment cannot build the release image; Task 1 bash truthy check + grep still satisfy CI-less acceptance for this change.

- [ ] **Step 3: Final commit only if Step 2 forced doc/script tweaks; otherwise done**

If no further edits, skip commit.

---

## Spec coverage checklist

| Spec requirement | Task |
|------------------|------|
| Single vault process / no server flag | (implicit — no server task) |
| `DEMO_DATA` default true; truthy parsing | Task 1 |
| Skip seed when DB exists | Task 1 |
| Compose `DEMO_DATA` | Task 2 |
| Docs list in spec | Task 3 |
| No `VAULT_MODE` compat alias | Tasks 1–3 |
| Acceptance grep / compose behavior | Task 4 |
| Out of scope: `reset-demo` / config.toml / re-seed existing | honored (no tasks) |

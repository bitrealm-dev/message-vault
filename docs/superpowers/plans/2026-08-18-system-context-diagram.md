# System Context Diagram Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the collapsed one-box Message Vault context view with a C4 Level 1 System Context diagram whose members are the User, the Desktop app, the Webpage, and the Message Vault Server.

**Architecture:** C4 System Context shows one software system in scope and everything that talks to it from outside that boundary. The system in scope is the Message Vault Server (the local Axum process on `127.0.0.1:8080`). The Desktop app and Webpage are neighboring software systems: they are separate processes the server does not own as internal modules. The User is the only person. Phone backup files are omitted because they connect to the Desktop app, not to the server.

**Tech Stack:** C4-PlantUML (current `master` include, or the vendored bundle in `.cursor/skills/c4-diagrams/assets/includes/C4/`), PlantUML 1.2026.6 at `~/.local/share/plantuml/plantuml.jar`, GraphViz `dot` 14.1.2.

## Global Constraints

- Scope is the System Context diagram only. Do not rewrite the container, component, or deployment `.puml` files in this plan.
- Keep sources under `docs/maintainers/architecture/puml/` (existing maintainer location). Do not move diagrams to `docs/diagrams/`.
- Members: Person `User`; neighboring systems `Desktop app` and `Webpage`; system in scope `Message Vault Server`. No other boxes.
- Communication style: complete sentences in titles and descriptions. No review-note shorthand.
- Render with PlantUML 1.2026.6. Do not `apt install plantuml` (Ubuntu’s package is 1.2020.2).
- Keep an explicit `!include` of C4_Context so Cursor’s PlantUML preview works without `scripts/render.sh`.

## File map

| File | Role |
|------|------|
| `docs/maintainers/architecture/puml/vault_system_diagram.puml` | C4 System Context source. Replace the current one-box Message Vault view. |
| `docs/maintainers/architecture/System diagram.png` | Rendered PNG used next to the other architecture screenshots. Overwrite after a successful render. |
| `docs/maintainers/README.md` | Already links to `architecture/puml/`. Only change if the link text still describes a one-box product view. |

---

### Task 1: Rewrite the System Context PlantUML source

**Files:**
- Modify: `docs/maintainers/architecture/puml/vault_system_diagram.puml`

**Interfaces:**
- Consumes: C4-PlantUML `Person`, `System`, `System_Ext`, `Rel_R`, `Lay_D`, `LAYOUT_LANDSCAPE`, `LAYOUT_WITH_LEGEND`
- Produces: aliases `user`, `desktop`, `webpage`, `server` used by Task 2’s render command

- [ ] **Step 1: Write a failing membership check**

Create a throwaway check that the source names the four members. Run it against the current file so it fails before the rewrite:

```bash
python3 - <<'PY'
from pathlib import Path
p = Path("docs/maintainers/architecture/puml/vault_system_diagram.puml").read_text()
required = [
    'Person(user, "User"',
    'System_Ext(desktop, "Desktop app"',
    'System_Ext(webpage, "Webpage"',
    'System(server, "Message Vault Server"',
]
missing = [s for s in required if s not in p]
assert not missing, f"missing members: {missing}"
print("membership ok")
PY
```

Expected: FAIL with `missing members` (the current file uses `Owner of phone backups` and a single `Message Vault` system).

- [ ] **Step 2: Replace `vault_system_diagram.puml` with this exact source**

```plantuml
@startuml
!include https://raw.githubusercontent.com/plantuml-stdlib/C4-PlantUML/master/C4_Context.puml

LAYOUT_LANDSCAPE()
left to right direction
LAYOUT_WITH_LEGEND()

title System Context: Message Vault Server

Person(user, "User", "Extracts phone backups, imports them into the vault, and browses messages on this machine. No cloud account.")

System_Ext(desktop, "Desktop app", "Tauri native window. Extract and Format write files on disk. Import and browse call the vault HTTP API.")
System_Ext(webpage, "Webpage", "Same React SPA in a browser at http://127.0.0.1:8080.")
System(server, "Message Vault Server", "Local HTTP API and static files on 127.0.0.1:8080. Started with cargo run -p message-vault-server -- serve.")

Rel_R(user, desktop, "Uses")
Rel_R(user, webpage, "Uses")
Rel_R(desktop, server, "Calls HTTP /v1", "http://127.0.0.1:8080")
Rel_R(webpage, server, "Calls GET / and /v1", "http://127.0.0.1:8080")
Lay_D(desktop, webpage)

@enduml
```

Why these types: `System(server, ...)` is the system in scope (solid border). `System_Ext` marks the Desktop app and Webpage as neighboring processes outside that server boundary. `Person` is the only human actor.

Why these relationships: the User operates the two UIs; only those UIs talk to the server. The server does not read phone backups.

- [ ] **Step 3: Re-run the membership check**

Run the same Python snippet from Step 1.

Expected: `membership ok`

- [ ] **Step 4: Confirm Cursor preview include**

The first non-comment line after `@startuml` must remain:

```
!include https://raw.githubusercontent.com/plantuml-stdlib/C4-PlantUML/master/C4_Context.puml
```

Do not switch this file to “no include / render.sh preload only.” The jebbs PlantUML extension renders with `plantuml.jar` and will not inject the skill’s vendored bundle.

- [ ] **Step 5: Commit**

```bash
git add docs/maintainers/architecture/puml/vault_system_diagram.puml
git commit -m "$(cat <<'EOF'
docs(c4): show server context with user and neighboring UIs

The old context view collapsed the product into one box. Level 1 now
names the User, Desktop app, Webpage, and Message Vault Server so the
server boundary is visible.
EOF
)"
```

---

### Task 2: Render PNG and replace the architecture screenshot

**Files:**
- Modify: `docs/maintainers/architecture/System diagram.png`
- Modify: `docs/maintainers/README.md` (only if the C4 bullet still implies a one-box product diagram)

**Interfaces:**
- Consumes: `vault_system_diagram.puml` from Task 1
- Produces: PNG at `docs/maintainers/architecture/System diagram.png`

- [ ] **Step 1: Prove the current PNG is stale**

```bash
python3 - <<'PY'
from pathlib import Path
puml = Path("docs/maintainers/architecture/puml/vault_system_diagram.puml").read_text()
assert 'System(server, "Message Vault Server"' in puml
png = Path("docs/maintainers/architecture/System diagram.png")
assert png.exists() and png.stat().st_size > 0
print(f"png bytes={png.stat().st_size} mtime={png.stat().st_mtime}")
PY
```

Note the byte size. After render it must change.

- [ ] **Step 2: Render with PlantUML 1.2026.6**

```bash
java -jar "$HOME/.local/share/plantuml/plantuml.jar" -version
# First line must contain: PlantUML version 1.2026.6

java -jar "$HOME/.local/share/plantuml/plantuml.jar" \
  -tpng \
  "docs/maintainers/architecture/puml/vault_system_diagram.puml"
```

PlantUML writes `docs/maintainers/architecture/puml/vault_system_diagram.png` next to the source.

If that command fails with undefined C4 macros, retry with the skill preload (same jar, vendored includes):

```bash
java -jar "$HOME/.local/share/plantuml/plantuml.jar" \
  -DRELATIVE_INCLUDE="$PWD/.cursor/skills/c4-diagrams/assets/includes/C4" \
  -I "$PWD/.cursor/skills/c4-diagrams/assets/includes/C4/C4_All.puml" \
  -tpng \
  "docs/maintainers/architecture/puml/vault_system_diagram.puml"
```

Expected: file `docs/maintainers/architecture/puml/vault_system_diagram.png` exists and is larger than 1 KB.

- [ ] **Step 3: Install the PNG as the maintainer screenshot**

```bash
cp "docs/maintainers/architecture/puml/vault_system_diagram.png" \
   "docs/maintainers/architecture/System diagram.png"
```

Do not commit `vault_system_diagram.png` as a second copy. Delete the sibling PNG after the copy so the folder stays “source `.puml` plus the named screenshots”:

```bash
rm "docs/maintainers/architecture/puml/vault_system_diagram.png"
```

- [ ] **Step 4: Visual check**

Open `docs/maintainers/architecture/System diagram.png` in the editor image viewer. Required:

- Four boxes only: User, Desktop app, Webpage, Message Vault Server
- User is a Person
- Message Vault Server is the in-scope software system (solid border)
- Desktop app and Webpage are external/neighboring systems (dashed border)
- Edges: User → Desktop app “Uses”; User → Webpage “Uses”; Desktop app → Server “Calls HTTP /v1”; Webpage → Server “Calls GET / and /v1”
- Landscape / left-to-right, not a single vertical chain
- No overlapping labels

If the layout is a tall stack, add `Lay_R(user, desktop)` before the `Rel_*` lines, re-render, and repeat this step.

- [ ] **Step 5: README line**

If `docs/maintainers/README.md` still says the context view is a one-box product, change the C4 bullet to:

```markdown
- [C4 diagrams](architecture/puml/) — from-source system context (User, Desktop app, Webpage, Message Vault Server), plus container, component, and deployment views. Preview the `.puml` files with current C4-PlantUML.
```

If it already points at the puml folder without claiming a one-box product, leave it unchanged.

- [ ] **Step 6: Commit**

```bash
git add "docs/maintainers/architecture/System diagram.png" docs/maintainers/README.md
git commit -m "$(cat <<'EOF'
docs(c4): render system context PNG for the server boundary

Replace the old one-box screenshot with the rendered Level 1 diagram
so the User, Desktop app, Webpage, and Message Vault Server are visible.
EOF
)"
```

# VPS setup (Ubuntu 26.04 + Cloudflare + Hanko + Docker Hub)

End-to-end runbook for `https://app.bitrealm.dev`. The Ansible playbook under
[`ansible/`](../../ansible/) provisions the host; this doc covers the console
clicks and secrets that Ansible does not create for you.

## Prerequisites

- Fresh **Ubuntu 26.04** VPS with SSH (`ubuntu` or your sudo user)
- Domain on Cloudflare (`bitrealm.dev`)
- Hanko Cloud organization (use a **prod** project, separate from local)
- Docker Hub image `mbeisser1/message-vault` (CI publishes on `v*` tags)
- Local machine with Ansible (`ansible-playbook`) and this git checkout

Canonical app URL: **`https://app.bitrealm.dev`** (not apex/`www` unless you
intentionally serve the vault there).

## 1. Cloudflare

### DNS

1. Cloudflare → domain → **DNS**.
2. Add / confirm **`app`** → VPS public IP, proxy **on** (orange cloud).
3. Optional: `www` → apex redirect (Page Rules / Redirect Rules). Apex marketing
   site is out of scope; do not put the vault on apex unless intentional.

### SSL / TLS

1. **SSL/TLS** → Overview → encryption mode **Full (strict)**.
2. **SSL/TLS** → **Origin Server** → **Create certificate**.
3. Hostnames: at least `app.bitrealm.dev` (include `bitrealm.dev` /
   `www.bitrealm.dev` if those names stay in nginx `server_name`).
4. Save the PEM cert and private key. On the VPS:

```bash
sudo mkdir -p /etc/ssl/cloudflare
sudo tee /etc/ssl/cloudflare/bitrealm.dev.pem >/dev/null <<'EOF'
-----BEGIN CERTIFICATE-----
...origin certificate...
-----END CERTIFICATE-----
EOF
sudo tee /etc/ssl/cloudflare/bitrealm.dev.key >/dev/null <<'EOF'
-----BEGIN PRIVATE KEY-----
...private key...
-----END PRIVATE KEY-----
EOF
sudo chmod 750 /etc/ssl/cloudflare
sudo chmod 640 /etc/ssl/cloudflare/bitrealm.dev.key
sudo chmod 644 /etc/ssl/cloudflare/bitrealm.dev.pem
```

Compose mounts these into nginx at `/etc/nginx/certs/` (see
[`compose-hub.yml`](../../compose-hub.yml)).

### Security notes

- Do **not** put Cloudflare Zero Trust / Access in front of the same app if
  Hanko is the login (double auth).
- Bot Fight / aggressive WAF can break auth POSTs or Hanko element traffic. If
  login fails only through Cloudflare, check **Security → Events** and loosen
  rules for `/api/auth/*`.

## 2. Hanko (production project)

1. [Hanko Cloud](https://cloud.hanko.io) → create / open the **prod** project
   (keep a separate project for `http://localhost:3000`).
2. **Settings → URLs**:
   - **App URL:** `https://app.bitrealm.dev`
   - **Allowed origins:** `https://app.bitrealm.dev` (App URL is included
     automatically; add others only if needed).
3. Dashboard → copy the **API URL**
   (`https://xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx.hanko.io`).
4. You will put that URL into Ansible `group_vars/vault.yml` and into the
   GitHub Actions secret used at image build time (next section).

## 3. Publish Hub image (bake Hanko URL)

`NEXT_PUBLIC_HANKO_API_URL` is baked into the Next.js client at **image build**
time. Runtime env on the VPS alone is not enough for the login widget in Hub
images.

1. GitHub repo → **Settings → Secrets and variables → Actions**.
2. Add secret **`HANKO_API_URL`** = prod Hanko API URL (same as above).
3. Tag a release (`v*`) or run the **Docker** workflow. CI passes the secret as
   Docker build-arg `NEXT_PUBLIC_HANKO_API_URL` (see
   [`.github/workflows/docker.yml`](../../.github/workflows/docker.yml)).
4. Confirm the image appears on Docker Hub as `mbeisser1/message-vault` (tag /
   `latest`).

Optional pin on the VPS: `VAULT_IMAGE=mbeisser1/message-vault:v0.1.0`.

## 4. Ansible bring-up

On your **control machine** (this repo):

```bash
cd ansible
cp inventory/hosts.example inventory/hosts
cp group_vars/vault.yml.example group_vars/vault.yml
```

Edit `inventory/hosts`:

```ini
[vault]
app ansible_host=YOUR.VPS.IP ansible_user=ubuntu
```

Edit `group_vars/vault.yml`:

```yaml
vault_docker_user: ubuntu
vault_image: mbeisser1/message-vault:latest
vault_mode: personal
vault_auth: hanko
hanko_api_url: https://YOUR-PROJECT.hanko.io
```

Install origin certs on the VPS (section 1) **before** the playbook’s compose
step. Then:

```bash
cd ansible
ansible-playbook playbooks/site.yml
```

What the playbook does:

1. Apt refresh, base packages, UFW (`22` / `80` / `443`).
2. Official Docker Engine + Compose plugin.
3. `/opt/message-vault-rs` with `compose-hub.yml`, `nginx/conf.d/`, rendered
   `.env`; asserts Cloudflare certs exist; `docker compose … pull && up -d`.

Vault UI/API bind to **localhost** on the VPS; public traffic goes through
nginx on `80`/`443` (Cloudflare → origin).

## 5. Verify

```bash
# From your laptop
curl -sSI https://app.bitrealm.dev/login | head
curl -sS https://app.bitrealm.dev/health

# On the VPS
cd /opt/message-vault-rs
sudo docker compose -f compose-hub.yml ps
curl -sS http://127.0.0.1:8080/health
```

Browser:

1. Open `https://app.bitrealm.dev/login` — Hanko widget (not local user/pass).
2. Sign up / sign in → onboarding (display name + phone) on first vault account.
3. Settings → Access → generate Import API token for exporters.

VPS must reach Hanko over HTTPS (JWKS). No Cloudflare tunnel change needed for
that egress.

## 6. Day-2 operations

### Update the image

```bash
# After CI publishes a new tag / latest
cd /opt/message-vault-rs
# optional: edit VAULT_IMAGE in .env
sudo docker compose -f compose-hub.yml pull
sudo docker compose -f compose-hub.yml up -d
```

Or re-run Ansible after bumping `vault_image` in `group_vars/vault.yml`.

### Staging drop folder

Host path `/opt/message-vault-rs/staging` → container `/app/staging`. Copy JSONL
exports there when using folder ingest.

### Backup vault data

Named volume `vault-data` holds SQLite and account files. Example backup:

```bash
sudo docker compose -f compose-hub.yml stop vault
sudo docker run --rm \
  -v message-vault-rs_vault-data:/data:ro \
  -v "$PWD:/backup" \
  alpine tar czf /backup/vault-data-$(date -u +%Y%m%d).tgz -C /data .
sudo docker compose -f compose-hub.yml start vault
```

(Volume name may be prefixed by the Compose project directory name; check with
`docker volume ls | grep vault-data`.)

### Auth reminder

Wiping the DB does not delete Hanko users. After a wipe, users sign in with
Hanko again; the vault auto-provisions a new account and sends them through
onboarding. Clear stale `mv_account_id` cookies if needed.

## Related

- Compose file: [`compose-hub.yml`](../../compose-hub.yml)
- Nginx: [`nginx/conf.d/default.conf`](../../nginx/conf.d/default.conf)
- Docker (all modes): [get-started/docker](https://bitrealm-dev.github.io/message-vault-rs/get-started/docker/)
- Local Hanko: `web/.env.local` with `VAULT_AUTH=hanko` (separate Hanko project)

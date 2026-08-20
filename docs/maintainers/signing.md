# Code signing for Windows and macOS releases

Release builds ship **unsigned** until the GitHub repository secrets below are configured. The [Release workflow](../../.github/workflows/release.yml) already contains gated signing steps: they stay skipped while the certificate secrets are empty, and activate automatically on the next run once secrets exist.

End-user install docs still warn about SmartScreen / Gatekeeper until signing is turned on. After the first signed release, update those warnings in [`install-the-desktop-app.md`](../src/content/docs/vault/user/get-started/install-the-desktop-app.md) and the workflow release notes.

## What to obtain

### Windows

A code-signing certificate that can produce a `.pfx` (PKCS#12) file:

| Option | Notes |
|--------|--------|
| **OV (Organization Validation)** code-signing cert | Cheapest common path. SmartScreen may still warn until the certificate builds reputation. |
| **EV (Extended Validation)** code-signing cert | Instant SmartScreen reputation, but keys usually live on a hardware token / HSM — not a plain exportable `.pfx` suitable for GitHub Actions without extra tooling. |
| **Microsoft Azure Trusted Signing** | Cloud signing without shipping a private key as a repo secret. Requires different Actions steps than the `.pfx` path below; use that product’s docs if choosing this route. |

The workflow’s Windows step expects an exportable `.pfx` plus its password (OV-style). EV / Azure Trusted Signing need a different integration.

### macOS

1. An [Apple Developer Program](https://developer.apple.com/programs/) membership (paid annually).
2. A **Developer ID Application** certificate from [Certificates, Identifiers & Profiles](https://developer.apple.com/account/resources/certificates/list). Export it as a `.p12` with a password.
3. An **app-specific password** for the Apple ID used for notarization (appleid.apple.com → Sign-In and Security → App-Specific Passwords), **or** an App Store Connect API key (not wired in the current workflow; the workflow uses Apple ID + app-specific password).

**Stapling caveat:** Apple can staple a notarization ticket onto a `.app` bundle, `.pkg`, or `.dmg`. A bare Mach-O executable (what the release ZIP ships today) can be signed and notarized, but Gatekeeper still needs one online check on first launch. Packaging the GUI as a proper `.app` is a separate follow-up if offline-verifiable double-click installs are required.

## GitHub secrets to add

Repository **Settings → Secrets and variables → Actions**. Base64-encode certificate files so they fit in a secret:

```bash
# Linux / macOS
base64 -w0 code-sign.pfx > windows-cert.b64   # GNU coreutils
base64 -i DeveloperID.p12 | tr -d '\n' > macos-cert.b64   # macOS
```

### Windows

| Secret | Value |
|--------|--------|
| `WINDOWS_CERTIFICATE_BASE64` | Base64 of the `.pfx` file (single line, no newlines) |
| `WINDOWS_CERTIFICATE_PASSWORD` | Password protecting the `.pfx` |

### macOS

| Secret | Value |
|--------|--------|
| `MACOS_CERTIFICATE_BASE64` | Base64 of the Developer ID Application `.p12` |
| `MACOS_CERTIFICATE_PASSWORD` | Password protecting the `.p12` |
| `MACOS_SIGNING_IDENTITY` | Exact codesign identity string, e.g. `Developer ID Application: Example Org (ABCD1234XY)` (run `security find-identity -v -p codesigning` after importing locally) |
| `NOTARY_APPLE_ID` | Apple ID email used for notarization |
| `NOTARY_TEAM_ID` | 10-character Team ID |
| `NOTARY_APP_SPECIFIC_PASSWORD` | App-specific password (not the Apple ID login password) |

## What the workflow does once secrets exist

1. **Build** release binaries as today.
2. **Windows** (`windows-latest` only, when `WINDOWS_CERTIFICATE_BASE64` is non-empty): decode the `.pfx`, locate `signtool.exe`, sign each project `.exe` under `target/release/` with SHA-256 and an RFC 3161 timestamp, then package.
3. **macOS** (`macos-latest` only, when `MACOS_CERTIFICATE_BASE64` is non-empty): import the `.p12` into a temporary keychain, `codesign --options runtime --timestamp` each project binary, submit a zip of `message-vault` to `notarytool --wait`, tear down the keychain, then package.
4. **Package** still runs `scripts/deprecated/package-release.sh` (GUI at root; `lib/` for ffmpeg/ffprobe; `cli/` for wtsexporter only; `licenses/`). Third-party helpers are not re-signed by these steps.

No further workflow edits are required to enable signing — only the secrets.

## Local smoke test (optional)

After configuring secrets, cut a pre-release version (for example `0.0.0-sign-test`), download the Windows/macOS archives, and verify:

- Windows: right-click an `.exe` → Properties → Digital Signatures.
- macOS: `codesign -dv --verbose=4 ./message-vault` and `spctl -a -vv ./message-vault` (online Gatekeeper check).

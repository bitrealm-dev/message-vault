"use client";

import { formatPhoneDisplay } from "@/lib/phoneE164";
import { useCallback, useEffect, useRef, useState } from "react";
import { useRouter } from "next/navigation";
import { DeleteAccountDialog } from "./DeleteAccountDialog";
import { ChevronRightIcon, EllipsisIcon } from "./icons";
import { useDismissible } from "./useDismissible";

type AccountEmail = {
  email: string;
  isPrimary: boolean;
};

type AccountData = {
  id: string;
  username: string;
  primaryEmail: string;
  emails: AccountEmail[];
  readOnly: boolean;
  isDemo: boolean;
  apiToken: string;
  vaultOwner: {
    firstName: string;
    lastName: string;
    displayName: string;
    phones: string[];
  };
};

function normalizeEmail(email: string): string {
  return email.trim().toLowerCase();
}

export function SettingsAccountForm() {
  const router = useRouter();
  const [data, setData] = useState<AccountData | null>(null);
  const [username, setUsername] = useState("");
  const [primaryEmail, setPrimaryEmail] = useState("");
  const [emails, setEmails] = useState<AccountEmail[]>([]);
  const [newEmail, setNewEmail] = useState("");
  const [readOnly, setReadOnly] = useState(false);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [deletingMessages, setDeletingMessages] = useState(false);
  const [deleteDialogOpen, setDeleteDialogOpen] = useState(false);
  const [dangerZoneOpen, setDangerZoneOpen] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [saved, setSaved] = useState(false);
  const [apiToken, setApiToken] = useState("");
  const [tokenCopied, setTokenCopied] = useState(false);
  const [regeneratingToken, setRegeneratingToken] = useState(false);

  const applyAccount = (json: AccountData) => {
    setData(json);
    setUsername(json.username);
    setPrimaryEmail(json.primaryEmail);
    setEmails(json.emails);
    setReadOnly(json.readOnly);
    setApiToken(json.apiToken ?? "");
  };

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const res = await fetch("/api/settings/account");
      const json = (await res.json()) as AccountData & { error?: string };
      if (!res.ok) throw new Error(json.error ?? "Failed to load account");
      applyAccount(json);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to load account");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  const updatePrimaryEmail = (value: string) => {
    setPrimaryEmail(value);
    setEmails((current) =>
      current.map((entry) =>
        entry.isPrimary ? { ...entry, email: value } : entry,
      ),
    );
  };

  const addEmail = () => {
    const trimmed = newEmail.trim();
    if (!trimmed) return;
    if (emails.some((entry) => normalizeEmail(entry.email) === normalizeEmail(trimmed))) {
      setError("That email is already on this account");
      return;
    }
    setEmails((current) => [...current, { email: trimmed, isPrimary: false }]);
    setNewEmail("");
    setError(null);
  };

  const removeEmail = (email: string) => {
    const entry = emails.find((item) => item.email === email);
    if (!entry || entry.isPrimary) return;
    setEmails((current) => current.filter((item) => item.email !== email));
  };

  const makePrimary = (email: string) => {
    setEmails((current) =>
      current.map((entry) => ({
        email: entry.email,
        isPrimary: entry.email === email,
      })),
    );
    setPrimaryEmail(email);
  };

  const save = async () => {
    setSaving(true);
    setError(null);
    setSaved(false);
    try {
      const res = await fetch("/api/settings/account", {
        method: "PATCH",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ username, readOnly, emails }),
      });
      const json = (await res.json()) as AccountData & { error?: string };
      if (!res.ok) throw new Error(json.error ?? "Save failed");
      applyAccount(json);
      setSaved(true);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Save failed");
    } finally {
      setSaving(false);
    }
  };

  const copyApiToken = async () => {
    if (!apiToken) return;
    try {
      await navigator.clipboard.writeText(apiToken);
      setTokenCopied(true);
      window.setTimeout(() => setTokenCopied(false), 2000);
    } catch {
      setError("Could not copy token to clipboard");
    }
  };

  const regenerateApiToken = async () => {
    if (
      !window.confirm(
        "Regenerate API token? The old token will stop working for vault-push immediately.",
      )
    ) {
      return;
    }
    setRegeneratingToken(true);
    setError(null);
    try {
      const res = await fetch("/api/settings/account", {
        method: "PATCH",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ regenerateApiToken: true }),
      });
      const json = (await res.json()) as AccountData & { error?: string };
      if (!res.ok) throw new Error(json.error ?? "Regenerate failed");
      applyAccount(json);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Regenerate failed");
    } finally {
      setRegeneratingToken(false);
    }
  };

  const performDelete = async () => {
    setDeleting(true);
    setError(null);
    try {
      const res = await fetch("/api/settings/account", { method: "DELETE" });
      const json = (await res.json()) as { ok?: boolean; error?: string };
      if (!res.ok || !json.ok) {
        throw new Error(json.error ?? "Delete failed");
      }
      setDeleteDialogOpen(false);
      router.replace("/login");
      router.refresh();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Delete failed");
    } finally {
      setDeleting(false);
    }
  };

  const deleteAllMessages = async () => {
    setDeletingMessages(true);
    setError(null);
    try {
      const res = await fetch("/api/settings/messages", { method: "DELETE" });
      const json = (await res.json()) as { ok?: boolean; error?: string };
      if (!res.ok || !json.ok) {
        throw new Error(json.error ?? "Delete messages failed");
      }
      router.refresh();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Delete messages failed");
    } finally {
      setDeletingMessages(false);
    }
  };

  if (loading) {
    return <p className="text-[14px] text-muted">Loading…</p>;
  }

  const additionalEmails = emails.filter((entry) => !entry.isPrimary);

  return (
    <div className="max-w-xl space-y-10">
      <section>
        <h2 className="text-[12px] font-semibold tracking-wider text-muted uppercase">
          Sign-in details
        </h2>
        <p className="mt-1 text-[13px] text-muted">
          Credentials for logging into Message Vault in the browser.
        </p>

        <div className="mt-4 space-y-4">
          <label className="block">
            <span className="text-[13px] text-text">Username</span>
            <input
              type="text"
              value={username}
              onChange={(e) => setUsername(e.target.value)}
              className="mt-1 w-full rounded-md border border-border bg-elevated px-3 py-2 text-[14px] text-text outline-none focus:border-accent"
            />
          </label>

          <label className="block">
            <span className="text-[13px] text-text">Primary email</span>
            <input
              type="email"
              value={primaryEmail}
              onChange={(e) => updatePrimaryEmail(e.target.value)}
              className="mt-1 w-full rounded-md border border-border bg-elevated px-3 py-2 text-[14px] text-text outline-none focus:border-accent"
            />
          </label>

          <div>
            <span className="text-[13px] text-text">Additional emails</span>
            {additionalEmails.length > 0 ? (
              <ul className="mt-2 space-y-2">
                {additionalEmails.map((entry) => (
                  <AdditionalEmailRow
                    key={entry.email}
                    email={entry.email}
                    onMakePrimary={() => makePrimary(entry.email)}
                    onRemove={() => removeEmail(entry.email)}
                  />
                ))}
              </ul>
            ) : (
              <p className="mt-1 text-[12px] text-muted">None</p>
            )}

            <div className="mt-3 flex gap-2">
              <input
                type="email"
                value={newEmail}
                onChange={(e) => setNewEmail(e.target.value)}
                placeholder="Add another email…"
                className="min-w-0 flex-1 rounded-md border border-border bg-elevated px-3 py-2 text-[14px] text-text outline-none placeholder:text-muted focus:border-accent"
              />
              <button
                type="button"
                onClick={addEmail}
                className="shrink-0 rounded-md border border-border bg-elevated px-3 py-2 text-[13px] text-text transition-colors hover:bg-hover"
              >
                Add
              </button>
            </div>
          </div>
        </div>
      </section>

      <section>
        <h2 className="text-[12px] font-semibold tracking-wider text-muted uppercase">
          Vault access
        </h2>
        <p className="mt-1 text-[13px] text-muted">
          Control whether this browser session can change vault data.
        </p>
        <label className="mt-4 flex items-start gap-2.5">
          <input
            type="checkbox"
            checked={readOnly}
            onChange={(e) => setReadOnly(e.target.checked)}
            className="mt-0.5 size-4 rounded border-border accent-accent"
          />
          <span>
            <span className="block text-[13px] text-text">Read-only mode</span>
            <span className="block text-[12px] text-muted">
              Prevent edits and destructive actions in the Web UI. Imports
              through the API or CLI remain available.
            </span>
          </span>
        </label>
      </section>

      <section>
        <h2 className="text-[12px] font-semibold tracking-wider text-muted uppercase">
          Import API token
        </h2>
        <p className="mt-1 text-[13px] text-muted">
          Paste this into vault-push or vault-push-gui. It identifies your account by username —
          you do not need an account UUID. This is not your website login.
        </p>
        <div className="mt-3 flex flex-col gap-2 sm:flex-row sm:items-center">
          <code className="min-w-0 flex-1 break-all rounded-md border border-border bg-elevated px-3 py-2 font-mono text-[12px] text-text">
            {apiToken || "—"}
          </code>
          <div className="flex shrink-0 gap-2">
            <button
              type="button"
              disabled={!apiToken || regeneratingToken}
              onClick={() => void copyApiToken()}
              className="rounded-md border border-border bg-elevated px-3 py-2 text-[13px] text-text transition-colors hover:bg-hover disabled:opacity-50"
            >
              {tokenCopied ? "Copied" : "Copy"}
            </button>
            <button
              type="button"
              disabled={regeneratingToken || deleting}
              onClick={() => void regenerateApiToken()}
              className="rounded-md border border-border bg-elevated px-3 py-2 text-[13px] text-text transition-colors hover:bg-hover disabled:opacity-50"
            >
              {regeneratingToken ? "…" : "Regenerate"}
            </button>
          </div>
        </div>
      </section>

      {data && (
        <section>
          <h2 className="text-[12px] font-semibold tracking-wider text-muted uppercase">
            Vault identity
          </h2>
          <p className="mt-1 text-[13px] text-muted">
            Name and phones used when matching your own messages. Reingest after
            changing these values.
          </p>

          <dl className="mt-4 space-y-3 text-[14px]">
            <div>
              <dt className="text-[12px] text-muted">First name</dt>
              <dd className="mt-0.5 text-text">
                {data.vaultOwner.firstName || "—"}
              </dd>
            </div>
            <div>
              <dt className="text-[12px] text-muted">Last name</dt>
              <dd className="mt-0.5 text-text">
                {data.vaultOwner.lastName || "—"}
              </dd>
            </div>
            <div>
              <dt className="text-[12px] text-muted">Phones</dt>
              <dd className="mt-0.5 text-text">
                {data.vaultOwner.phones.length > 0 ? (
                  <ul className="list-disc list-outside pl-4">
                    {data.vaultOwner.phones.map((phone) => (
                      <li key={phone}>{formatPhoneDisplay(phone)}</li>
                    ))}
                  </ul>
                ) : (
                  "None"
                )}
              </dd>
              <p className="mt-1 text-[12px] text-muted">
                Stored in E.164 format.
              </p>
            </div>
          </dl>
        </section>
      )}

      <div className="flex items-center gap-3 border-t border-border pt-6">
        <button
          type="button"
          disabled={saving || deleting}
          onClick={() => void save()}
          className="rounded-md border border-border bg-elevated px-4 py-2 text-[13px] text-text transition-colors hover:bg-hover disabled:opacity-50"
        >
          {saving ? "Saving…" : "Save changes"}
        </button>
        {saved && <span className="text-[13px] text-muted">Saved.</span>}
        {error && (
          <span className="text-[13px] text-danger" role="alert">
            {error}
          </span>
        )}
      </div>

      {data && (
        <section className="border-t border-border pt-8">
          <button
            type="button"
            aria-expanded={dangerZoneOpen}
            onClick={() => setDangerZoneOpen((open) => !open)}
            className="flex w-full items-center gap-2 text-left"
          >
            <ChevronRightIcon
              className={`size-4 shrink-0 text-danger/80 transition-transform ${
                dangerZoneOpen ? "rotate-90" : ""
              }`}
            />
            <span className="text-[12px] font-semibold tracking-wider text-danger uppercase">
              Danger zone
            </span>
          </button>

          {dangerZoneOpen && (
            <div className="mt-4 space-y-4 pl-6">
              <div className="flex items-center justify-between gap-4">
                <p className="min-w-0 flex-1 text-[13px] text-muted">
                  Permanently delete every conversation, message, and attachment
                  for this account. Contacts, labels, and login details remain.
                </p>
                <button
                  type="button"
                  disabled={saving || deleting || deletingMessages}
                  onClick={() => void deleteAllMessages()}
                  className="shrink-0 rounded-md border border-red-500/40 bg-red-500/15 px-4 py-2 text-[13px] text-red-100 transition-colors hover:bg-red-500/25 disabled:opacity-50"
                >
                  {deletingMessages ? "Deleting…" : "Delete all messages"}
                </button>
              </div>

              {!data.isDemo ? (
                <div className="flex items-center justify-between gap-4">
                  <p className="min-w-0 flex-1 text-[13px] text-muted">
                    Permanently delete this account and all vault data tied to it.
                  </p>
                  <button
                    type="button"
                    disabled={saving || deleting || deletingMessages}
                    onClick={() => setDeleteDialogOpen(true)}
                    className="shrink-0 rounded-md border border-red-500/40 bg-red-500/15 px-4 py-2 text-[13px] text-red-100 transition-colors hover:bg-red-500/25 disabled:opacity-50"
                  >
                    Delete account
                  </button>
                </div>
              ) : null}
            </div>
          )}
        </section>
      )}

      <DeleteAccountDialog
        open={deleteDialogOpen}
        username={data?.username ?? username}
        deleting={deleting}
        onClose={() => {
          if (!deleting) setDeleteDialogOpen(false);
        }}
        onConfirm={() => void performDelete()}
      />
    </div>
  );
}

function AdditionalEmailRow({
  email,
  onMakePrimary,
  onRemove,
}: {
  email: string;
  onMakePrimary: () => void;
  onRemove: () => void;
}) {
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLLIElement>(null);

  useDismissible({
    open,
    onDismiss: () => setOpen(false),
    refs: [rootRef],
  });

  return (
    <li
      ref={rootRef}
      className="relative flex items-center justify-between gap-3 rounded-md border border-border bg-elevated px-3 py-2"
    >
      <span className="min-w-0 truncate text-[14px] text-text">{email}</span>
      <button
        type="button"
        aria-label={`Actions for ${email}`}
        aria-expanded={open}
        onClick={() => setOpen((value) => !value)}
        className="flex h-7 w-7 shrink-0 items-center justify-center rounded-md text-muted transition-colors hover:bg-hover hover:text-text"
      >
        <EllipsisIcon className="size-5" />
      </button>
      {open && (
        <div className="absolute top-full right-0 z-50 mt-1 min-w-[10.5rem] rounded-xl border border-border bg-popover py-1 shadow-xl">
          <button
            type="button"
            className="flex w-full px-3 py-1.5 text-left text-[13px] text-text hover:bg-hover-strong"
            onClick={() => {
              setOpen(false);
              onMakePrimary();
            }}
          >
            Make primary
          </button>
          <button
            type="button"
            className="flex w-full px-3 py-1.5 text-left text-[13px] text-text hover:bg-hover-strong"
            onClick={() => {
              setOpen(false);
              onRemove();
            }}
          >
            Remove
          </button>
        </div>
      )}
    </li>
  );
}

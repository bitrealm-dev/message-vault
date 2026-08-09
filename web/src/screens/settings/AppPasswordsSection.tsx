import { useCallback, useEffect, useState } from "react";
import { apiClient } from "../../lib/api";
import Button from "../../components/Button";
import AppPasswordRevealDialog from "../../components/AppPasswordRevealDialog";
import { inputStyle, sectionTitle } from "./profileStyles";

type AppPasswordScopes = "import" | "export" | "both";

type AppPasswordItem = {
  id: string;
  label: string;
  scopes: AppPasswordScopes | string;
  created_at: string;
};

function formatCreatedAt(secs: string): string {
  const n = Number(secs);
  if (!Number.isFinite(n) || n <= 0) return secs;
  try {
    return new Date(n * 1000).toLocaleString();
  } catch {
    return secs;
  }
}

function scopesLabel(scopes: string): string {
  switch (scopes) {
    case "import":
      return "Import";
    case "export":
      return "Export";
    case "both":
      return "Import + export";
    default:
      return scopes;
  }
}

/** Named CLI credentials (import/export). Separate from the rotating GUI session token. */
export function AppPasswordsSection() {
  const [items, setItems] = useState<AppPasswordItem[]>([]);
  const [loadError, setLoadError] = useState("");
  const [busy, setBusy] = useState(false);
  const [label, setLabel] = useState("");
  const [scopes, setScopes] = useState<AppPasswordScopes>("both");
  const [actionError, setActionError] = useState("");
  const [reveal, setReveal] = useState<{ label: string; token: string } | null>(null);

  const reload = useCallback(async () => {
    setLoadError("");
    try {
      const res = await apiClient.get<{ items: AppPasswordItem[] }>("/v1/account/app-passwords");
      setItems(res.items ?? []);
    } catch (e) {
      setLoadError(e instanceof Error ? e.message : String(e));
    }
  }, []);

  useEffect(() => {
    void reload();
  }, [reload]);

  const create = async () => {
    const trimmed = label.trim();
    if (!trimmed) return;
    setBusy(true);
    setActionError("");
    try {
      const res = await apiClient.post<{
        id: string;
        label: string;
        scopes: string;
        created_at: string;
        token: string;
      }>("/v1/account/app-passwords", { label: trimmed, scopes });
      setLabel("");
      setScopes("both");
      setReveal({ label: res.label, token: res.token });
      await reload();
    } catch (e) {
      setActionError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const revoke = async (item: AppPasswordItem) => {
    if (!confirm(`Revoke app password “${item.label}”? CLI tools using it will stop working.`)) {
      return;
    }
    setBusy(true);
    setActionError("");
    try {
      await apiClient.delete(`/v1/account/app-passwords/${encodeURIComponent(item.id)}`);
      await reload();
    } catch (e) {
      setActionError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div style={{ marginBottom: "1.5rem" }}>
      <h3 style={sectionTitle}>App passwords</h3>
      <p style={{ margin: "0 0 0.75rem", fontSize: "0.813rem", color: "var(--muted)" }}>
        Long-lived secrets for CLI tools such as vault-push and vault-pull. Choose import,
        export, or both when creating one. Signing in to the GUI uses a separate session
        token that changes on each login and does not revoke these passwords.
      </p>

      {loadError && (
        <div style={{ fontSize: "0.813rem", color: "var(--danger)", marginBottom: "0.75rem" }}>
          {loadError}
        </div>
      )}

      {items.length === 0 ? (
        <div style={{ fontSize: "0.875rem", color: "var(--muted)", marginBottom: "0.75rem" }}>
          No app passwords yet.
        </div>
      ) : (
        <div style={{ marginBottom: "0.75rem" }}>
          {items.map((item) => (
            <div
              key={item.id}
              style={{
                display: "flex",
                alignItems: "center",
                gap: "0.75rem",
                padding: "0.375rem 0",
                borderBottom: "1px solid var(--border)",
                fontSize: "0.875rem",
              }}
            >
              <span style={{ flex: 1, minWidth: 0, fontWeight: 500 }}>{item.label}</span>
              <span style={{ color: "var(--muted)", fontSize: "0.75rem", flexShrink: 0 }}>
                {scopesLabel(item.scopes)}
              </span>
              <span style={{ color: "var(--muted)", fontSize: "0.75rem", flexShrink: 0 }}>
                {formatCreatedAt(item.created_at)}
              </span>
              <Button
                variant="ghost"
                disabled={busy}
                onClick={() => void revoke(item)}
                style={{
                  fontSize: "0.813rem",
                  padding: "0.2rem 0.5rem",
                  color: "var(--danger)",
                }}
              >
                Revoke
              </Button>
            </div>
          ))}
        </div>
      )}

      <div
        style={{
          display: "flex",
          gap: "0.5rem",
          flexWrap: "wrap",
          alignItems: "center",
          marginBottom: "0.35rem",
        }}
      >
        <input
          type="text"
          value={label}
          onChange={(e) => setLabel(e.target.value)}
          placeholder="Label (e.g. laptop CLI)"
          disabled={busy}
          style={{ ...inputStyle, flex: 1, minWidth: "12rem" }}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              e.preventDefault();
              void create();
            }
          }}
        />
        <select
          value={scopes}
          onChange={(e) => setScopes(e.target.value as AppPasswordScopes)}
          disabled={busy}
          aria-label="App password access"
          style={{ ...inputStyle, width: "auto", minWidth: "9.5rem" }}
        >
          <option value="both">Import + export</option>
          <option value="import">Import only</option>
          <option value="export">Export only</option>
        </select>
        <Button
          variant="primary"
          disabled={busy || !label.trim()}
          onClick={() => void create()}
          style={{ padding: "0.35rem 0.85rem" }}
        >
          Create
        </Button>
      </div>
      {actionError && (
        <div style={{ fontSize: "0.813rem", color: "var(--danger)" }} role="alert">
          {actionError}
        </div>
      )}

      <AppPasswordRevealDialog
        open={reveal !== null}
        label={reveal?.label ?? ""}
        token={reveal?.token ?? ""}
        onClose={() => setReveal(null)}
      />
    </div>
  );
}

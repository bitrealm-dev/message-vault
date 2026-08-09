import { useCallback, useEffect, useState } from "react";
import { apiClient } from "../../lib/api";
import Button from "../../components/Button";
import ApiTokenRevealDialog from "../../components/ApiTokenRevealDialog";
import { inputStyle, sectionTitle } from "./profileStyles";

type ApiTokenScopes = "import" | "export" | "both";

type ApiTokenItem = {
  id: string;
  label: string;
  scopes: ApiTokenScopes | string;
  /** Masked secret, e.g. `mv-api-Sd1**********mE`. */
  token_hint: string;
  created_at: string;
  /** Unix seconds string, or null/absent if never used. */
  last_accessed_at?: string | null;
};

function formatTimestamp(secs: string | null | undefined): string {
  if (secs == null || secs === "") return "Never";
  const n = Number(secs);
  if (!Number.isFinite(n) || n <= 0) return "Never";
  try {
    return new Date(n * 1000).toLocaleString();
  } catch {
    return "Never";
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

/** Named CLI API tokens (import/export). Separate from the rotating GUI session token. */
export function ApiTokensSection() {
  const [items, setItems] = useState<ApiTokenItem[]>([]);
  const [loadError, setLoadError] = useState("");
  const [busy, setBusy] = useState(false);
  const [label, setLabel] = useState("");
  const [scopes, setScopes] = useState<ApiTokenScopes>("both");
  const [actionError, setActionError] = useState("");
  const [reveal, setReveal] = useState<{ label: string; token: string } | null>(null);

  const reload = useCallback(async () => {
    setLoadError("");
    try {
      const res = await apiClient.get<{ items: ApiTokenItem[] }>("/v1/account/api-tokens");
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
      }>("/v1/account/api-tokens", { label: trimmed, scopes });
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

  const revoke = async (item: ApiTokenItem) => {
    if (!confirm(`Revoke API token “${item.label}”? CLI tools using it will stop working.`)) {
      return;
    }
    setBusy(true);
    setActionError("");
    try {
      await apiClient.delete(`/v1/account/api-tokens/${encodeURIComponent(item.id)}`);
      await reload();
    } catch (e) {
      setActionError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div style={{ marginBottom: "1.5rem" }}>
      <h3 style={sectionTitle}>API tokens</h3>
      <p style={{ margin: "0 0 0.75rem", fontSize: "0.813rem", color: "var(--muted)" }}>
        Long-lived secrets for CLI tools such as vault-push and vault-pull. Choose import,
        export, or both when creating one. Signing in to the GUI uses a separate session
        token that changes on each login and does not revoke these tokens.
      </p>

      {loadError && (
        <div style={{ fontSize: "0.813rem", color: "var(--danger)", marginBottom: "0.75rem" }}>
          {loadError}
        </div>
      )}

      <div
        style={{
          display: "flex",
          gap: "0.5rem",
          flexWrap: "wrap",
          alignItems: "center",
          marginBottom: "0.75rem",
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
          onChange={(e) => setScopes(e.target.value as ApiTokenScopes)}
          disabled={busy}
          aria-label="API token access"
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
        <div style={{ fontSize: "0.813rem", color: "var(--danger)", marginBottom: "0.75rem" }} role="alert">
          {actionError}
        </div>
      )}

      {items.length === 0 ? (
        <div style={{ fontSize: "0.875rem", color: "var(--muted)" }}>
          No API tokens yet.
        </div>
      ) : (
        <div>
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
              <div style={{ flex: 1, minWidth: 0 }}>
                <div style={{ fontWeight: 500 }}>{item.label}</div>
                <div
                  style={{
                    fontFamily:
                      "ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace",
                    fontSize: "0.75rem",
                    color: "var(--muted)",
                    marginTop: "0.125rem",
                  }}
                  title="Masked API token"
                >
                  {item.token_hint || "mv-api-**********"}
                </div>
              </div>
              <span style={{ color: "var(--muted)", fontSize: "0.75rem", flexShrink: 0 }}>
                {scopesLabel(item.scopes)}
              </span>
              <div
                style={{
                  color: "var(--muted)",
                  fontSize: "0.75rem",
                  flexShrink: 0,
                  textAlign: "right",
                  lineHeight: 1.35,
                }}
              >
                <div>Created {formatTimestamp(item.created_at)}</div>
                <div>Last accessed {formatTimestamp(item.last_accessed_at)}</div>
              </div>
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

      <ApiTokenRevealDialog
        open={reveal !== null}
        label={reveal?.label ?? ""}
        token={reveal?.token ?? ""}
        onClose={() => setReveal(null)}
      />
    </div>
  );
}

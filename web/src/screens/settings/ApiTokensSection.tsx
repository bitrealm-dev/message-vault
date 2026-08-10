import { useCallback, useEffect, useState } from "react";
import { apiClient } from "../../lib/api";
import Button from "../../components/Button";
import ApiTokenRevealDialog from "../../components/ApiTokenRevealDialog";
import ConfirmDialog from "../../components/ConfirmDialog";
import Select, { ListBoxItem, selectItemClassName } from "../../components/Select";
import { inputClassName, sectionTitleClass } from "./profileStyles";

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
  const [revokeTarget, setRevokeTarget] = useState<ApiTokenItem | null>(null);

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
    setBusy(true);
    setActionError("");
    try {
      await apiClient.delete(`/v1/account/api-tokens/${encodeURIComponent(item.id)}`);
      await reload();
    } catch (e) {
      setActionError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
      setRevokeTarget(null);
    }
  };

  return (
    <div className="mb-6">
      <h3 className={sectionTitleClass}>API tokens</h3>
      <p className="mb-3 text-[0.813rem] text-muted">
        API Tokens are authorization keys used to allow external vault tools to import and
        export message data.
      </p>

      {loadError && (
        <div className="mb-3 text-[0.813rem] text-danger">
          {loadError}
        </div>
      )}

      <div className="mb-3 flex flex-wrap items-center gap-2">
        <input
          type="text"
          value={label}
          onChange={(e) => setLabel(e.target.value)}
          placeholder="Label (e.g. laptop CLI)"
          disabled={busy}
          className={`${inputClassName} min-w-[12rem] flex-1`}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              e.preventDefault();
              void create();
            }
          }}
        />
        <Select
          selectedKey={scopes}
          onSelectionChange={(k) => setScopes(k as ApiTokenScopes)}
          isDisabled={busy}
          aria-label="API token access"
          className="shrink-0 min-w-[9.5rem]"
        >
          <ListBoxItem id="both" className={selectItemClassName}>Import + export</ListBoxItem>
          <ListBoxItem id="import" className={selectItemClassName}>Import only</ListBoxItem>
          <ListBoxItem id="export" className={selectItemClassName}>Export only</ListBoxItem>
        </Select>
        <Button
          variant="primary"
          disabled={busy || !label.trim()}
          onClick={() => void create()}
          className="!px-[0.85rem] !py-[0.35rem]"
        >
          Create
        </Button>
      </div>
      {actionError && (
        <div className="mb-3 text-[0.813rem] text-danger" role="alert">
          {actionError}
        </div>
      )}

      {items.length === 0 ? (
        <div className="text-[0.875rem] text-muted">
          No API tokens yet.
        </div>
      ) : (
        <div>
          {items.map((item) => (
            <div
              key={item.id}
              className="flex items-center gap-3 border-b border-border py-1.5 text-[0.875rem]"
            >
              <div className="min-w-0 flex-1">
                <div className="font-medium">{item.label}</div>
                <div
                  className="mt-0.5 font-mono text-[0.75rem] text-muted"
                  title="Masked API token"
                >
                  {item.token_hint || "mv-api-**********"}
                </div>
              </div>
              <span className="shrink-0 text-[0.75rem] text-muted">
                {scopesLabel(item.scopes)}
              </span>
              <div className="shrink-0 text-right text-[0.75rem] leading-[1.35] text-muted">
                <div>Created {formatTimestamp(item.created_at)}</div>
                <div>Last accessed {formatTimestamp(item.last_accessed_at)}</div>
              </div>
              <Button
                variant="ghost"
                disabled={busy}
                onClick={() => setRevokeTarget(item)}
                className="!px-2 !py-[0.2rem] !text-[0.813rem] !text-danger"
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

      <ConfirmDialog
        open={revokeTarget !== null}
        title="Revoke API token?"
        body={revokeTarget ? `Revoke API token “${revokeTarget.label}”? CLI tools using it will stop working.` : ""}
        confirmLabel="Revoke token"
        danger
        busy={busy}
        onClose={() => setRevokeTarget(null)}
        onConfirm={() => revokeTarget && void revoke(revokeTarget)}
      />
    </div>
  );
}

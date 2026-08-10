import { useCallback, useEffect, useState } from "react";
import {
  Table,
  TableHeader,
  TableBody,
  Column,
  Row,
  Cell,
} from "react-aria-components";
import { apiClient } from "../../lib/api";
import Button from "../../components/Button";
import TextField from "../../components/TextField";
import ModalShell from "../../components/ModalShell";
import ApiTokenRevealDialog from "../../components/ApiTokenRevealDialog";
import ConfirmDialog from "../../components/ConfirmDialog";

type ApiTokenItem = {
  id: string;
  label: string;
  scopes: string;
  /** Masked secret, e.g. `mv-api-Sd..mE`. */
  token_hint: string;
  created_at: string;
  /** Unix seconds string, or null/absent if never used. */
  last_accessed_at?: string | null;
};

/** Normalize stored hints (old `**********` or new `..`) to `mv-api-xx..yy`. */
function displayKeyHint(hint: string | null | undefined): string {
  const raw = (hint ?? "").trim();
  if (!raw) return "mv-api-..";
  if (/^(mv-api-|mv-app-).{2}\.\..{2}$/.test(raw)) return raw;
  const stars = raw.match(/^(mv-api-|mv-app-)(.{2}).*\*{2,}(.{2})$/);
  if (stars) return `${stars[1]}${stars[2]}..${stars[3]}`;
  return raw;
}

function formatDate(secs: string | null | undefined): string {
  if (secs == null || secs === "") return "Never";
  const n = Number(secs);
  if (!Number.isFinite(n) || n <= 0) return "Never";
  try {
    return new Date(n * 1000).toLocaleDateString();
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
      return "Import / Export";
    default:
      return scopes;
  }
}

function TrashIcon() {
  return (
    <svg
      width="13"
      height="13"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d="M3 6h18" />
      <path d="M8 6V4h8v2" />
      <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6" />
      <path d="M10 11v6" />
      <path d="M14 11v6" />
    </svg>
  );
}

function PencilIcon() {
  return (
    <svg
      width="13"
      height="13"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d="M12 20h9" />
      <path d="M16.5 3.5a2.1 2.1 0 0 1 3 3L7 19l-4 1 1-4Z" />
    </svg>
  );
}

const thClass =
  "px-3 py-2 text-left text-[0.75rem] font-bold text-muted";
const tdClass = "px-3 py-2 text-[0.75rem] text-text align-middle";
const tdMuted = "px-3 py-2 text-[0.75rem] text-muted align-middle";
const iconBtnClass =
  "!inline-flex !aspect-square !h-7 !w-7 !min-h-7 !min-w-7 !shrink-0 !items-center !justify-center !rounded-sm !p-0 !leading-none !text-muted";

/** Named CLI API keys (import/export). Separate from the rotating GUI session token. */
export function ApiTokensSection() {
  const [items, setItems] = useState<ApiTokenItem[]>([]);
  const [loadError, setLoadError] = useState("");
  const [busy, setBusy] = useState(false);
  const [composing, setComposing] = useState(false);
  const [label, setLabel] = useState("");
  const [actionError, setActionError] = useState("");
  const [reveal, setReveal] = useState<{ label: string; token: string } | null>(null);
  const [revokeTarget, setRevokeTarget] = useState<ApiTokenItem | null>(null);
  const [renameTarget, setRenameTarget] = useState<ApiTokenItem | null>(null);
  const [renameLabel, setRenameLabel] = useState("");

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

  const cancelCompose = () => {
    setComposing(false);
    setLabel("");
    setActionError("");
  };

  const openRename = (item: ApiTokenItem) => {
    setRenameTarget(item);
    setRenameLabel(item.label);
    setActionError("");
  };

  const closeRename = () => {
    if (busy) return;
    setRenameTarget(null);
    setRenameLabel("");
  };

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
      }>("/v1/account/api-tokens", { label: trimmed, scopes: "both" });
      setLabel("");
      setComposing(false);
      setReveal({ label: res.label, token: res.token });
      await reload();
    } catch (e) {
      setActionError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const rename = async () => {
    if (!renameTarget) return;
    const trimmed = renameLabel.trim();
    if (!trimmed) return;
    setBusy(true);
    setActionError("");
    try {
      await apiClient.patch(`/v1/account/api-tokens/${encodeURIComponent(renameTarget.id)}`, {
        label: trimmed,
      });
      setRenameTarget(null);
      setRenameLabel("");
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
      <div className="mb-3 flex flex-wrap items-center justify-between gap-2">
        <h3 className="mb-0 text-[0.75rem] font-bold text-text">API keys</h3>
        {!composing && (
          <Button
            variant="secondary"
            disabled={busy}
            onClick={() => setComposing(true)}
            className="!rounded-md !border-transparent !bg-text !px-3 !py-1 !text-[0.75rem] !font-semibold !text-bg hover:!brightness-90"
          >
            Add
          </Button>
        )}
      </div>

      {loadError && (
        <div className="mb-3 text-[0.75rem] text-danger">{loadError}</div>
      )}
      {actionError && (
        <div className="mb-3 text-[0.75rem] text-danger" role="alert">
          {actionError}
        </div>
      )}

      {composing && (
        <div className="mb-3 flex flex-wrap items-end gap-2 rounded-xl border border-border bg-elevated p-3">
          <TextField
            value={label}
            onChange={setLabel}
            placeholder="Enter API key name…"
            isDisabled={busy}
            aria-label="API key name"
            className="min-w-[12rem] flex-1"
            onKeyDown={(e) => {
              if (e.key === "Enter") {
                e.preventDefault();
                void create();
              }
              if (e.key === "Escape") {
                e.preventDefault();
                cancelCompose();
              }
            }}
          />
          <span className="pb-2.5 text-[0.75rem] text-muted">Import / Export</span>
          <Button
            variant="secondary"
            disabled={busy || !label.trim()}
            onClick={() => void create()}
            className="!px-3 !py-1.5 !text-[0.75rem]"
          >
            Save
          </Button>
          <Button
            variant="secondary"
            disabled={busy}
            onClick={cancelCompose}
            className="!px-3 !py-1.5 !text-[0.75rem]"
          >
            Cancel
          </Button>
        </div>
      )}

      <div className="overflow-hidden rounded-xl border border-border bg-elevated">
        <Table
          aria-label="API keys"
          selectionMode="none"
          className="w-full table-fixed border-collapse text-left outline-none"
        >
          <TableHeader className="border-b border-border">
            <Column isRowHeader className={`${thClass} w-[22%]`}>
              Name
            </Column>
            <Column className={`${thClass} w-[24%]`}>Key</Column>
            <Column className={`${thClass} w-[18%]`}>Scope</Column>
            <Column className={`${thClass} w-[14%]`}>Created</Column>
            <Column className={`${thClass} w-[14%]`}>Last Used</Column>
            <Column className={`${thClass} w-[8%]`} />
          </TableHeader>
          <TableBody
            items={items}
            dependencies={[busy]}
            renderEmptyState={() =>
              composing ? null : (
                <div className="px-5 py-6 text-[0.75rem] text-muted">No API keys yet.</div>
              )
            }
            className="outline-none"
          >
            {(item) => (
              <Row
                id={item.id}
                className="border-b border-border last:border-b-0 outline-none"
              >
                <Cell className={`${tdClass} truncate font-medium`}>
                  <span className="block truncate" title={item.label}>
                    {item.label}
                  </span>
                </Cell>
                <Cell className={`${tdMuted} truncate font-mono text-[0.688rem]`}>
                  <span className="block truncate" title="Masked API key">
                    {displayKeyHint(item.token_hint)}
                  </span>
                </Cell>
                <Cell className={tdClass}>{scopesLabel(item.scopes)}</Cell>
                <Cell className={tdMuted}>{formatDate(item.created_at)}</Cell>
                <Cell className={tdMuted}>{formatDate(item.last_accessed_at)}</Cell>
                <Cell className={`${tdClass}`}>
                  <div className="flex items-center justify-end gap-1">
                    <Button
                      variant="secondary"
                      disabled={busy}
                      title="Edit API Key"
                      aria-label="Edit API Key"
                      onClick={() => openRename(item)}
                      className={`${iconBtnClass} hover:!text-text`}
                    >
                      <PencilIcon />
                    </Button>
                    <Button
                      variant="secondary"
                      disabled={busy}
                      title="Revoke API Key"
                      aria-label="Revoke API Key"
                      onClick={() => setRevokeTarget(item)}
                      className={`${iconBtnClass} hover:!text-danger`}
                    >
                      <TrashIcon />
                    </Button>
                  </div>
                </Cell>
              </Row>
            )}
          </TableBody>
        </Table>
      </div>

      <p className="mt-3 text-[0.75rem] leading-relaxed text-muted">
        API keys give secure, programmatic access so vault tools can import and export
        message data. Treat them like passwords: keep them private and never share them
        publicly.
      </p>

      <ApiTokenRevealDialog
        open={reveal !== null}
        label={reveal?.label ?? ""}
        token={reveal?.token ?? ""}
        onClose={() => setReveal(null)}
      />

      <ModalShell
        open={renameTarget !== null}
        onOpenChange={(o) => {
          if (!o) closeRename();
        }}
        dismissable={!busy}
        label="Rename API key"
        maxWidth="24rem"
      >
        <button
          type="button"
          aria-label="Close"
          disabled={busy}
          onClick={closeRename}
          className="absolute top-3 right-3 cursor-pointer border-none bg-transparent text-[1.25rem] leading-none text-muted disabled:cursor-not-allowed disabled:opacity-50"
        >
          ×
        </button>
        <h2 className="mb-2 pr-6 text-[1.125rem] font-semibold text-text">Rename API key</h2>
        <p className="mb-3 text-[0.813rem] text-muted">
          Choose a name you will recognize later. The secret value does not change.
        </p>
        <TextField
          label="Name"
          value={renameLabel}
          onChange={setRenameLabel}
          isDisabled={busy}
          autoFocus
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              e.preventDefault();
              void rename();
            }
          }}
        />
        <div className="mt-5 flex justify-end gap-2">
          <Button onPress={closeRename} isDisabled={busy}>
            Cancel
          </Button>
          <Button
            variant="primary"
            onPress={() => void rename()}
            isDisabled={busy || !renameLabel.trim()}
          >
            {busy ? "Saving…" : "Save"}
          </Button>
        </div>
      </ModalShell>

      <ConfirmDialog
        open={revokeTarget !== null}
        title="Delete API key?"
        body={
          revokeTarget
            ? `Delete API key “${revokeTarget.label}”? CLI tools using it will stop working.`
            : ""
        }
        confirmLabel="Delete key"
        danger
        busy={busy}
        onClose={() => setRevokeTarget(null)}
        onConfirm={() => revokeTarget && void revoke(revokeTarget)}
      />
    </div>
  );
}

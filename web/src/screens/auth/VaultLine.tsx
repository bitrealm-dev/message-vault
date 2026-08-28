import Button from "../../components/Button";
import HealthDot from "../../components/HealthDot";
import TextField from "../../components/TextField";
import type { VaultHealthStatus } from "../../lib/vaultHealth";

/** How the sign-in card is getting on with the vault it resolved. */
export type VaultConnection = "connecting" | "connected" | "editing" | "disconnected";

export interface VaultLineProps {
  state: VaultConnection;
  /** Host to name, never blank — a blank address shows this page's host. */
  host: string;
  /** Address being typed while the editor is open. */
  draft: string;
  health: VaultHealthStatus;
  onDraftChange: (value: string) => void;
  onEdit: () => void;
  onCancel: () => void;
  onSubmit: () => void;
}

const STATUS_WORD: Record<VaultConnection, string> = {
  connecting: "connecting…",
  connected: "connected",
  editing: "connecting…",
  disconnected: "disconnected",
};

const STATUS_COLOR: Record<VaultConnection, string> = {
  connecting: "text-muted",
  connected: "text-ok",
  editing: "text-muted",
  disconnected: "text-danger",
};

/**
 * The vault a sign-in card is talking to: its host, its connection state, and
 * the way to point somewhere else. The address sits above the password rather
 * than one screen back, so it is in front of you while you type.
 */
export default function VaultLine({
  state,
  host,
  draft,
  health,
  onDraftChange,
  onEdit,
  onCancel,
  onSubmit,
}: VaultLineProps) {
  const open = state === "editing" || state === "disconnected";

  return (
    <div className="mb-3.5">
      <div className="flex items-center gap-1.5 text-[0.75rem] text-muted">
        <span className="min-w-0 truncate font-medium text-text">{host}</span>
        <span aria-hidden="true">·</span>
        <span className={STATUS_COLOR[state]} aria-live="polite">
          {STATUS_WORD[state]}
        </span>
        <span className="ml-auto flex items-center gap-2">
          {open ? <HealthDot status={health} /> : null}
          {state === "connected" ? (
            <Button variant="ghost" size="xs" onPress={onEdit}>
              Change
            </Button>
          ) : null}
          {state === "editing" ? (
            <Button variant="ghost" size="xs" onPress={onCancel}>
              Cancel
            </Button>
          ) : null}
        </span>
      </div>

      {open ? (
        <>
          <div className="mt-2 flex gap-2">
            <TextField
              aria-label="Vault address"
              value={draft}
              onChange={onDraftChange}
              onKeyDown={(e) => e.key === "Enter" && onSubmit()}
              placeholder="https://vault.example.com"
              className="min-w-0 flex-1"
            />
            <Button variant="secondary" size="sm" onPress={onSubmit} className="shrink-0">
              {state === "disconnected" ? "Retry" : "Use"}
            </Button>
          </div>
          <p className="mt-1 text-[0.75rem] text-muted">
            Start your vault, or enter another address.
          </p>
        </>
      ) : null}
    </div>
  );
}

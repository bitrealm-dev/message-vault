/** How the auth card is getting on with the vault it resolved. */
export type VaultConnection = "connecting" | "connected" | "disconnected";

const WORD: Record<VaultConnection, string> = {
  connecting: "Connecting",
  connected: "Connected",
  disconnected: "Disconnected",
};

/**
 * The word carries the state on its own — there is no indicator dot. Connecting
 * keeps the slow flash the old light had, moved onto the text as an opacity
 * pulse, because scaling type wobbles the baseline underneath it.
 */
const TONE: Record<VaultConnection, string> = {
  connecting: "text-text motion-safe:animate-pulse",
  connected: "text-ok",
  disconnected: "text-danger",
};

export interface VaultStatusProps {
  state: VaultConnection;
  className?: string;
}

/**
 * One word naming the vault connection. It replaces the old host-and-dot line:
 * the address is a setting, not something to read on every visit, so what is
 * left is the only part a person acts on.
 */
export default function VaultStatus({ state, className }: VaultStatusProps) {
  return (
    <p role="status" className={`text-[0.813rem] font-medium ${TONE[state]} ${className ?? ""}`}>
      {WORD[state]}
    </p>
  );
}

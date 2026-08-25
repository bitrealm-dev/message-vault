import type { VaultHealthStatus } from "../lib/vaultHealth";
import { healthStatusLabel } from "../lib/vaultHealth";

const statusClass: Record<VaultHealthStatus, string> = {
  unknown: "bg-muted",
  ok: "bg-ok",
  fail: "bg-danger",
};

/** Small grey / green / red light for vault reachability next to Server URL. */
export default function HealthDot({ status }: { status: VaultHealthStatus }) {
  const label = healthStatusLabel(status);
  return (
    <span
      role="status"
      aria-label={label}
      title={label}
      className={`inline-block size-2 shrink-0 rounded-full transition-colors duration-300 ${statusClass[status]}`}
    />
  );
}

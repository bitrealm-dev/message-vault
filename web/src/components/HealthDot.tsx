import type { VaultHealthStatus } from "../lib/vaultHealth";
import { healthStatusLabel } from "../lib/vaultHealth";

const statusClass: Record<VaultHealthStatus, string> = {
  unknown: "bg-muted",
  checking: "bg-muted motion-safe:animate-pulse",
  ok: "bg-ok",
  fail: "bg-danger",
};

/** Small grey / green / red light for vault reachability, next to the vault line's address field. */
export default function HealthDot({ status }: { status: VaultHealthStatus }) {
  const label = healthStatusLabel(status);
  return (
    <span role="status" title={label} className="inline-flex items-center">
      <span
        aria-hidden
        className={`inline-block size-2.5 shrink-0 rounded-full shadow-[inset_0_0_0_1px_color-mix(in_srgb,var(--text)_35%,transparent)] transition-colors duration-300 motion-reduce:transition-none ${statusClass[status]}`}
      />
      <span className="sr-only">{label}</span>
    </span>
  );
}

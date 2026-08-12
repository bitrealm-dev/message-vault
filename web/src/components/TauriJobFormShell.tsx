import type { ReactNode } from "react";
import { isTauri } from "../lib/tauri-check";
import BackToLoginLink from "./BackToLoginLink";
import Button from "./Button";
import ProgressBar from "./ProgressBar";

export type TauriJobFormShellProps = {
  title: string;
  children: ReactNode;
  startLabel: string;
  runningLabel?: string;
  onStart: () => void;
  onCancel: () => void;
  running: boolean;
  startDisabled?: boolean;
  log: string[];
  error?: string | null;
  success?: ReactNode;
  requireTauri?: boolean;
  onBack?: () => void;
  intro?: ReactNode;
};

export default function TauriJobFormShell({
  title,
  children,
  startLabel,
  runningLabel,
  onStart,
  onCancel,
  running,
  startDisabled = false,
  log,
  error,
  success,
  requireTauri,
  onBack,
  intro,
}: TauriJobFormShellProps) {
  if (requireTauri && !isTauri()) {
    return (
      <div className="max-w-[700px] p-6 text-muted">
        Export requires the desktop app.
      </div>
    );
  }

  const disabled = running || startDisabled;

  return (
    <div className="max-w-[700px] p-6">
      <BackToLoginLink onBack={onBack} />
      <h2 className="m-0 mb-6">{title}</h2>
      {intro}
      {children}

      <div className="mt-6 flex gap-3">
        <Button
          variant="primary"
          onClick={onStart}
          disabled={disabled}
          className="!px-6 !py-2"
        >
          {running ? (runningLabel ?? `${startLabel}…`) : startLabel}
        </Button>
        <Button onClick={onCancel} disabled={!running} className="!px-6 !py-2">
          Cancel
        </Button>
      </div>

      {error ? (
        <div className="mt-4 rounded border border-danger-soft-border bg-danger-soft-bg p-3 text-[0.813rem] text-danger">
          {error}
        </div>
      ) : null}

      <div className="mt-6">
        <ProgressBar log={log} running={running} />
      </div>

      {success}
    </div>
  );
}

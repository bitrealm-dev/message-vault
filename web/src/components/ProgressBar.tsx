import { ProgressBar as RACProgressBar } from "react-aria-components";

interface ProgressBarProps {
  log: string[];
  running: boolean;
}

/**
 * Job progress indicator: an indeterminate React Aria progress bar while
 * `running` (indeterminate so screen readers announce it as one), plus the
 * job's log lines below. The `indeterminate` keyframes live in theme.css
 * `@layer base`.
 */
export default function ProgressBar({ log, running }: ProgressBarProps) {
  return (
    <div>
      {running && (
        <div className="mb-2">
          <RACProgressBar isIndeterminate className="w-full">
            <div className="h-2 w-full overflow-hidden rounded bg-border">
              <div className="h-full w-full rounded bg-accent animate-[indeterminate_1.5s_ease-in-out_infinite]" />
            </div>
          </RACProgressBar>
        </div>
      )}
      {log.length > 0 && (
        <pre className="m-0 max-h-[300px] overflow-auto whitespace-pre-wrap break-words rounded bg-hover p-2 text-[0.75rem]">
          {log.join("\n")}
        </pre>
      )}
    </div>
  );
}

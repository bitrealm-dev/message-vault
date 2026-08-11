import { Checkbox } from "react-aria-components";
import ThemeSettings from "../../components/ThemeSettings";
import { setUseNameAliases, useNameAliases } from "../../lib/useNameAliases";

function CheckIcon({ className }: { className?: string }) {
  return (
    <svg
      viewBox="0 0 16 16"
      fill="none"
      stroke="currentColor"
      strokeWidth="2.25"
      strokeLinecap="round"
      strokeLinejoin="round"
      className={className}
      aria-hidden="true"
    >
      <path d="M3.5 8.5 6.5 11.5 12.5 4.5" />
    </svg>
  );
}

export function AppearanceSection() {
  const useAliases = useNameAliases();

  return (
    <div>
      <Checkbox
        isSelected={useAliases}
        onChange={setUseNameAliases}
        className="mb-6 flex cursor-pointer items-start gap-2.5 text-[0.875rem] text-text outline-none data-focus-visible:ring-2 data-focus-visible:ring-accent"
      >
        {({ isSelected }) => (
          <>
            <span
              className={`mt-0.5 flex h-4 w-4 shrink-0 items-center justify-center rounded border ${
                isSelected
                  ? "border-accent bg-accent text-[color:var(--checkbox-check)]"
                  : "border-border bg-elevated"
              }`}
            >
              {isSelected ? <CheckIcon className="h-3.5 w-3.5" /> : null}
            </span>
            <span>
              <span className="font-medium">Use name aliases</span>
              <span className="mt-0.5 block text-[0.813rem] text-muted">
                When enabled, threads and messages show each service identity’s
                imported name when one is set.
              </span>
            </span>
          </>
        )}
      </Checkbox>
      <ThemeSettings />
    </div>
  );
}

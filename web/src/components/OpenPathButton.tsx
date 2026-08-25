import { type MouseEvent, type ReactNode, useState } from "react";
import { openPathInExplorer } from "../lib/openPath";

type OpenPathButtonProps = {
  path: string;
  children: ReactNode;
  className?: string;
  title?: string;
};

/** Text button that opens a file or directory with the OS default handler. */
export default function OpenPathButton({ path, children, className, title }: OpenPathButtonProps) {
  const [error, setError] = useState<string | null>(null);

  async function onClick(event: MouseEvent<HTMLButtonElement>): Promise<void> {
    event.preventDefault();
    event.stopPropagation();
    setError(null);
    try {
      await openPathInExplorer(path);
    } catch (caught) {
      const message = caught instanceof Error ? caught.message : String(caught);
      setError(message || "Could not open path");
      console.error("Failed to open path", caught);
    }
  }

  return (
    <span className="inline-flex max-w-full flex-col items-start">
      <button type="button" onClick={onClick} title={title ?? path} className={className}>
        {children}
      </button>
      {error ? (
        <span className="mt-0.5 text-[0.75rem] text-danger" role="alert">
          {error}
        </span>
      ) : null}
    </span>
  );
}

import type { MouseEvent, ReactNode } from "react";
import { openPathInExplorer } from "../lib/openPath";

type OpenPathButtonProps = {
  path: string;
  children: ReactNode;
  className?: string;
  title?: string;
};

/** Text button that opens a file or directory in the OS explorer. */
export default function OpenPathButton({ path, children, className, title }: OpenPathButtonProps) {
  async function onClick(event: MouseEvent<HTMLButtonElement>): Promise<void> {
    event.preventDefault();
    event.stopPropagation();
    try {
      await openPathInExplorer(path);
    } catch (error) {
      console.error("Failed to open path", error);
    }
  }

  return (
    <button type="button" onClick={onClick} title={title ?? path} className={className}>
      {children}
    </button>
  );
}

import type { ReactNode } from "react";
import { Column } from "react-aria-components";
import {
  linkClass,
  mutedClass,
  thClass,
  thRightClass,
} from "./handleTableStyles";

export function SortableColumn({
  id,
  widthClass,
  align = "center",
  isRowHeader,
  children,
}: {
  id: string;
  widthClass: string;
  align?: "center" | "right";
  isRowHeader?: boolean;
  children: ReactNode;
}) {
  const justify = align === "right" ? "justify-end" : "justify-center";
  const textAlign = align === "right" ? "text-right" : "text-center";
  const headerAlign = align === "right" ? thRightClass : thClass;
  return (
    <Column
      id={id}
      isRowHeader={isRowHeader}
      allowsSorting
      className={`${headerAlign} ${widthClass}`}
    >
      {({ sortDirection }) => (
        <span className={`relative mx-auto inline-flex max-w-full items-center ${justify}`}>
          <span className={`${textAlign} leading-tight`}>{children}</span>
          <span
            aria-hidden="true"
            className={`absolute top-1/2 left-[calc(100%+0.25rem)] -translate-y-1/2 text-[0.55rem] leading-none ${
              sortDirection ? "text-accent" : "invisible"
            }`}
          >
            {sortDirection === "descending" ? "▼" : "▲"}
          </span>
        </span>
      )}
    </Column>
  );
}

export function CountCell({
  value,
  onClick,
}: {
  value: number;
  onClick?: () => void;
}) {
  const text = value.toLocaleString();
  if (value > 0 && onClick) {
    return (
      <button
        type="button"
        className={linkClass}
        onClick={onClick}
        aria-label={`Open ${text} threads`}
      >
        {text}
      </button>
    );
  }
  return <span className={value === 0 ? mutedClass : undefined}>{text}</span>;
}

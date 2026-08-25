import type { ReactNode } from "react";
import { Column, ColumnResizer, Group } from "react-aria-components";
import {
  columnResizerClass,
  linkClass,
  mutedClass,
  thClass,
  thLeftClass,
  thRightClass,
} from "./handleTableStyles";
import { headerLabelMinWidth } from "./headerLabelMinWidth";

export function SortableColumn({
  id,
  widthClass = "",
  align = "left",
  isRowHeader,
  allowsResizing = false,
  defaultWidth,
  minWidth,
  children,
}: {
  id: string;
  widthClass?: string;
  align?: "left" | "center" | "right";
  isRowHeader?: boolean;
  allowsResizing?: boolean;
  defaultWidth?: number | `${number}%` | `${number}fr`;
  minWidth?: number;
  children: ReactNode;
}) {
  const justify =
    align === "right" ? "justify-end" : align === "left" ? "justify-start" : "justify-center";
  const textAlign =
    align === "right" ? "text-right" : align === "left" ? "text-left" : "text-center";
  const headerAlign = align === "right" ? thRightClass : align === "left" ? thLeftClass : thClass;
  const resolvedMinWidth =
    minWidth ??
    (allowsResizing && typeof children === "string" ? headerLabelMinWidth(children) : undefined);

  return (
    <Column
      id={id}
      isRowHeader={isRowHeader}
      allowsSorting
      defaultWidth={defaultWidth}
      minWidth={resolvedMinWidth}
      className={`${headerAlign} ${widthClass}`.trim()}
    >
      {({ sortDirection }) => (
        <div className="relative flex w-full min-w-0 items-center">
          <Group
            role="presentation"
            className={`flex min-w-0 flex-1 items-center outline-none ${justify}`}
          >
            <span
              className={`relative ${textAlign} leading-tight ${sortDirection ? "text-accent" : "text-text"}`}
            >
              {children}
              {sortDirection ? (
                <span
                  aria-hidden="true"
                  className="absolute top-1/2 left-[calc(100%+0.125rem)] -translate-y-1/2 text-[0.55rem] leading-none text-accent"
                >
                  {sortDirection === "descending" ? "▼" : "▲"}
                </span>
              ) : null}
            </span>
          </Group>
          {allowsResizing ? <ColumnResizer className={columnResizerClass} /> : null}
        </div>
      )}
    </Column>
  );
}

export function CountCell({
  value,
  onClick,
  loading = false,
}: {
  value: number;
  onClick?: () => void;
  /** When true, show an em dash instead of a zeroed stub count. */
  loading?: boolean;
}) {
  if (loading) {
    return <span className={mutedClass}>—</span>;
  }
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

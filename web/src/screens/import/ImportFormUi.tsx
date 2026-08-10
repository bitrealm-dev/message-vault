import type { ReactNode } from "react";
import { parseDate } from "@internationalized/date";
import {
  Button,
  Calendar,
  CalendarCell,
  CalendarGrid,
  CalendarGridBody,
  CalendarGridHeader,
  CalendarHeaderCell,
  CalendarHeading,
  DateInput,
  DatePicker,
  DateSegment,
  Dialog,
  Disclosure,
  DisclosurePanel,
  Group,
  Label,
  Popover,
} from "react-aria-components";
import type { AttachmentMediaMode } from "../../lib/types";

export const ATTACHMENT_OPTIONS: { id: AttachmentMediaMode; label: string }[] = [
  { id: "copy", label: "Copy" },
  { id: "convert", label: "Convert" },
  { id: "compress", label: "Compress & Convert" },
  { id: "skip", label: "Skip" },
];

export const RESOLUTION_OPTIONS = ["720p", "1080p", "4k"];

export const fieldStyle =
  "box-border w-full rounded-md border border-border bg-bg px-[0.6rem] py-[0.4rem] text-[0.875rem] text-text";

export const hintStyle = "mt-1 text-[0.75rem] text-muted";

export const sectionGap = "mb-[1.1rem]";

function CalendarIcon() {
  return (
    <svg
      width="15"
      height="15"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden
    >
      <rect x="3" y="4" width="18" height="18" rx="2" />
      <path d="M16 2v4" />
      <path d="M8 2v4" />
      <path d="M3 10h18" />
    </svg>
  );
}

/**
 * Date field backed by React Aria's DatePicker + Calendar. Parent value is an
 * ISO YYYY-MM-DD string (or "" for no date); typed and picked dates convert
 * via @internationalized/date. Typing a date works segment-by-segment
 * (slashes are handled by the segments), and the calendar icon opens a
 * popover calendar.
 */
export function DateField({
  label,
  value,
  onChange,
}: {
  label: string;
  value: string;
  onChange: (value: string) => void;
}) {
  // parseDate throws on malformed input; parent state is only ever "" or a
  // valid ISO date written by this component, so guard anyway.
  const calendarValue = value && /^\d{4}-\d{2}-\d{2}$/.test(value) ? parseDate(value) : null;

  return (
    <div className="min-w-[10rem] flex-[1_1_12rem]">
      <DatePicker
        value={calendarValue}
        onChange={(date) => onChange(date ? date.toString() : "")}
      >
        <Label className="mb-1 block text-[0.875rem] font-medium text-text">{label}</Label>
        <Group className="flex items-center rounded border border-border bg-elevated px-2 py-1.5 focus-within:border-accent">
          <DateInput className="flex flex-1 outline-none">
            {(segment) => (
              <DateSegment
                segment={segment}
                className="rounded-sm px-0.5 text-[0.875rem] text-text outline-none data-[placeholder]:text-muted focus:bg-accent focus:text-sent-text"
              />
            )}
          </DateInput>
          <Button
            aria-label={`Pick ${label}`}
            className="ml-1 flex shrink-0 items-center justify-center rounded border-0 bg-transparent p-0.5 text-muted outline-none hover:text-accent"
          >
            <CalendarIcon />
          </Button>
        </Group>
        <Popover className="z-[100] rounded-md border border-border bg-popover p-2 shadow-md outline-none">
          <Dialog className="outline-none">
            <Calendar className="outline-none">
              <div className="flex items-center justify-between pb-2">
                <Button
                  slot="previous"
                  className="flex h-6 w-6 items-center justify-center rounded border-0 bg-transparent text-muted outline-none hover:text-accent"
                >
                  ‹
                </Button>
                <CalendarHeading className="text-[0.875rem] font-medium text-text" />
                <Button
                  slot="next"
                  className="flex h-6 w-6 items-center justify-center rounded border-0 bg-transparent text-muted outline-none hover:text-accent"
                >
                  ›
                </Button>
              </div>
              <CalendarGrid className="border-separate border-spacing-1">
                <CalendarGridHeader>
                  {(day) => (
                    <CalendarHeaderCell className="px-1 pb-1 text-center text-[0.75rem] text-muted">
                      {day}
                    </CalendarHeaderCell>
                  )}
                </CalendarGridHeader>
                <CalendarGridBody>
                  {(date) => (
                    <CalendarCell
                      date={date}
                      className={({ isHovered, isPressed, isSelected, isFocused, isDisabled, isOutsideMonth }) =>
                        "flex h-8 w-8 items-center justify-center rounded text-[0.875rem] outline-none " +
                        (isOutsideMonth || isDisabled ? "text-muted opacity-50" : "text-text") +
                        (isHovered || isPressed ? " bg-hover" : "") +
                        (isSelected ? " bg-accent text-sent-text" : "") +
                        (isFocused ? " ring-1 ring-accent ring-inset" : "")
                      }
                    />
                  )}
                </CalendarGridBody>
              </CalendarGrid>
            </Calendar>
          </Dialog>
        </Popover>
      </DatePicker>
    </div>
  );
}

export function StackedField({
  label,
  children,
  trailing,
}: {
  label: string;
  children: ReactNode;
  trailing?: ReactNode;
}) {
  return (
    <div className={sectionGap}>
      <div className="flex items-baseline gap-3">
        <Label className="mb-1 block flex-1 text-[0.875rem] font-medium text-text">{label}</Label>
        {trailing}
      </div>
      {children}
    </div>
  );
}

export function CollapsibleSection({
  title,
  open,
  onToggle,
  children,
}: {
  title: string;
  open: boolean;
  onToggle: () => void;
  children: ReactNode;
}) {
  return (
    <Disclosure
      isExpanded={open}
      onExpandedChange={onToggle}
      className={`block ${open ? "mb-3" : "mb-5"}`}
    >
      {({ isExpanded }) => (
        <>
          <Button
            slot="trigger"
            className="flex w-full cursor-pointer items-center gap-2 rounded-none border-0 border-b border-border bg-transparent p-0 pb-2 pt-1 text-left text-[0.9375rem] font-semibold text-text outline-none hover:text-accent"
          >
            <span
              aria-hidden
              className={`inline-block text-[0.75rem] leading-none text-muted transition-transform duration-150 ${isExpanded ? "rotate-90" : ""}`}
            >
              ▶
            </span>
            <span>{title}</span>
          </Button>
          <DisclosurePanel className="mt-3 ml-3 outline-none">{children}</DisclosurePanel>
        </>
      )}
    </Disclosure>
  );
}

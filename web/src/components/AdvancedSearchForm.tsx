"use client";

import {
  composeSearchQuery,
  type AdvancedSearchForm as FormState,
  type DateFilterInput,
  type DateFilterMode,
} from "@/lib/searchQuery";
import { useEffect, useState, type ReactNode } from "react";

const inputClass =
  "w-full rounded-md border border-border bg-elevated px-2.5 py-1.5 text-[13px] text-text outline-none placeholder:text-muted focus:border-accent";
const labelClass = "w-28 shrink-0 text-[13px] text-muted";

const NO_DATES: DateFilterInput = { mode: "any", from: "", to: "" };

export function AdvancedSearchForm({
  sources,
  labels,
  initialQuery,
  showContactOption = true,
  onSearch,
  onCancel,
}: {
  sources: string[];
  labels: string[];
  initialQuery: string;
  /** Contact-grouped results only make sense where contacts are listed. */
  showContactOption?: boolean;
  onSearch: (query: string) => void;
  onCancel: () => void;
}) {
  const [within, setWithin] = useState("");
  const [withPerson, setWithPerson] = useState("");
  const [hasWords, setHasWords] = useState("");
  const [doesntHave, setDoesntHave] = useState("");
  const [date, setDate] = useState<DateFilterInput>(NO_DATES);
  const [firstContact, setFirstContact] = useState<DateFilterInput>(NO_DATES);
  const [lastContact, setLastContact] = useState<DateFilterInput>(NO_DATES);
  const [showContact, setShowContact] = useState(false);
  const [conversationType, setConversationType] = useState<
    "any" | "group" | "individual"
  >("any");
  const [source, setSource] = useState("");
  const [hasAttachment, setHasAttachment] = useState(false);

  useEffect(() => {
    // Seed free-text into Has the words when opening with a plain query.
    if (initialQuery.trim() && !/[a-z]+:/i.test(initialQuery)) {
      setHasWords(initialQuery.trim());
    }
  }, [initialQuery]);

  const submit = () => {
    const form: FormState = {
      within: within || undefined,
      withPerson,
      hasWords,
      doesntHave,
      date,
      firstContact,
      lastContact,
      showContact: showContactOption && showContact,
      conversationType,
      source: source || undefined,
      hasAttachment,
    };
    onSearch(composeSearchQuery(form));
  };

  return (
    <div className="border-b border-border bg-panel px-3 py-3">
      <div className="space-y-2">
        <Field label="Within">
          <select
            className={inputClass}
            value={within}
            onChange={(e) => setWithin(e.target.value)}
          >
            <option value="">All contacts</option>
            {labels.map((name) => (
              <option key={name} value={name}>
                {name}
              </option>
            ))}
          </select>
        </Field>
        <Field label="With person">
          <input
            className={inputClass}
            value={withPerson}
            onChange={(e) => setWithPerson(e.target.value)}
            placeholder="Name or number"
          />
        </Field>
        <Field label="Has the words">
          <input
            className={inputClass}
            value={hasWords}
            onChange={(e) => setHasWords(e.target.value)}
          />
        </Field>
        <Field label="Doesn't have">
          <input
            className={inputClass}
            value={doesntHave}
            onChange={(e) => setDoesntHave(e.target.value)}
          />
        </Field>
        <DateRangeField label="Date" value={date} onChange={setDate} />
        <DateRangeField
          label="First contact"
          value={firstContact}
          onChange={setFirstContact}
        />
        <DateRangeField
          label="Last contact"
          value={lastContact}
          onChange={setLastContact}
        />
        <Field label="Message type">
          <select
            className={inputClass}
            value={conversationType}
            onChange={(e) =>
              setConversationType(
                e.target.value as "any" | "group" | "individual",
              )
            }
          >
            <option value="any">All conversations</option>
            <option value="individual">1-1 only</option>
            <option value="group">Group only</option>
          </select>
        </Field>
        <Field label="Source">
          <select
            className={inputClass}
            value={source}
            onChange={(e) => setSource(e.target.value)}
          >
            <option value="">Any source</option>
            {sources.map((s) => (
              <option key={s} value={s}>
                {s}
              </option>
            ))}
          </select>
        </Field>
        {showContactOption ? (
          <label className="flex items-center gap-2 pl-32 text-[13px] text-text">
            <input
              type="checkbox"
              checked={showContact}
              onChange={(e) => setShowContact(e.target.checked)}
              className="size-3.5 accent-accent"
            />
            Show contact
          </label>
        ) : null}
        <label className="flex items-center gap-2 pl-32 text-[13px] text-text">
          <input
            type="checkbox"
            checked={hasAttachment}
            onChange={(e) => setHasAttachment(e.target.checked)}
            className="size-3.5 accent-accent"
          />
          Has an attachment
        </label>
      </div>
      <div className="mt-3 flex items-center justify-end gap-2">
        <button
          type="button"
          onClick={onCancel}
          className="rounded-md border border-border bg-elevated px-3 py-1.5 text-[13px] text-text hover:bg-hover"
        >
          Cancel
        </button>
        <button
          type="button"
          onClick={submit}
          className="rounded-md bg-accent px-3 py-1.5 text-[13px] font-medium text-sent-text hover:opacity-90"
        >
          Search
        </button>
      </div>
    </div>
  );
}

/** Mode picker plus the one or two date inputs that mode needs. */
function DateRangeField({
  label,
  value,
  onChange,
}: {
  label: string;
  value: DateFilterInput;
  onChange: (next: DateFilterInput) => void;
}) {
  const showFrom = value.mode === "on-or-after" || value.mode === "between";
  const showTo = value.mode === "before" || value.mode === "between";

  return (
    <div className="flex items-start gap-2">
      <span className={`${labelClass} pt-1.5`}>{label}</span>
      <div className="min-w-0 flex-1 space-y-1.5">
        <select
          className={inputClass}
          value={value.mode}
          aria-label={`${label} comparison`}
          onChange={(e) =>
            onChange({ ...value, mode: e.target.value as DateFilterMode })
          }
        >
          <option value="any">Any time</option>
          <option value="on-or-after">On or after</option>
          <option value="before">Before</option>
          <option value="between">Between</option>
        </select>
        {showFrom || showTo ? (
          <div className="flex items-center gap-1.5">
            {showFrom ? (
              <input
                type="date"
                className={inputClass}
                aria-label={
                  value.mode === "between" ? `${label} start` : `${label} date`
                }
                value={value.from ?? ""}
                onChange={(e) => onChange({ ...value, from: e.target.value })}
              />
            ) : null}
            {value.mode === "between" ? (
              <span className="shrink-0 text-[12px] text-muted">to</span>
            ) : null}
            {showTo ? (
              <input
                type="date"
                className={inputClass}
                aria-label={
                  value.mode === "between" ? `${label} end` : `${label} date`
                }
                value={value.to ?? ""}
                onChange={(e) => onChange({ ...value, to: e.target.value })}
              />
            ) : null}
          </div>
        ) : null}
      </div>
    </div>
  );
}

function Field({
  label,
  children,
}: {
  label: string;
  children: ReactNode;
}) {
  return (
    <div className="flex items-center gap-2">
      <span className={labelClass}>{label}</span>
      <div className="min-w-0 flex-1">{children}</div>
    </div>
  );
}

"use client";

import {
  composeSearchQuery,
  formFromSearchQuery,
  type AdvancedSearchForm as FormState,
  type CountFilterInput,
  type CountComparator,
  type DateFilterInput,
  type DateFilterMode,
} from "@/lib/searchQuery";
import { useState, type ReactNode } from "react";

const inputClass =
  "w-full rounded-md border border-border bg-elevated px-2.5 py-1.5 text-[13px] text-text outline-none placeholder:text-muted focus:border-accent";
const labelClass = "w-28 shrink-0 text-[13px] text-muted";

const NO_DATES: DateFilterInput = { mode: "any", from: "", to: "" };

export function AdvancedSearchForm({
  sources,
  labels,
  initialQuery,
  onSearch,
  onCancel,
}: {
  sources: string[];
  labels: string[];
  initialQuery: string;
  onSearch: (query: string) => void;
  onCancel: () => void;
}) {
  // Panel remounts each open; hydrate once from the query bar string.
  const seed = formFromSearchQuery(initialQuery);
  const [mode, setMode] = useState<"contacts" | "messages">(seed.mode ?? "messages");
  const [within, setWithin] = useState(seed.within ?? "");
  const [handle, setHandle] = useState(seed.handle ?? "");
  const [withPerson, setWithPerson] = useState(seed.withPerson ?? "");
  const [hasWords, setHasWords] = useState(seed.hasWords ?? "");
  const [doesntHave, setDoesntHave] = useState(seed.doesntHave ?? "");
  const [date, setDate] = useState<DateFilterInput>(seed.date ?? NO_DATES);
  const [firstContact, setFirstContact] = useState<DateFilterInput>(
    seed.firstContact ?? NO_DATES,
  );
  const [lastContact, setLastContact] = useState<DateFilterInput>(
    seed.lastContact ?? NO_DATES,
  );
  const [groupCount, setGroupCount] = useState<CountFilterInput>(
    seed.groupCount ?? { comparator: "any", value: "" },
  );
  const [messageCount, setMessageCount] = useState<CountFilterInput>(
    seed.messageCount ?? { comparator: "any", value: "" },
  );
  const [conversationType, setConversationType] = useState<
    "any" | "group" | "individual"
  >(seed.conversationType ?? "any");
  const [source, setSource] = useState(seed.source ?? "");
  const [hasAttachment, setHasAttachment] = useState(!!seed.hasAttachment);

  const submit = () => {
    const form: FormState = {
      mode,
      within: within || undefined,
      handle,
      withPerson,
      hasWords,
      doesntHave,
      date,
      firstContact,
      lastContact,
      groupCount,
      messageCount,
      conversationType,
      source: source || undefined,
      hasAttachment,
    };
    onSearch(composeSearchQuery(form));
  };

  return (
    <div className="border-b border-border bg-panel px-3 py-3">
      <div
        role="tablist"
        aria-label="Search type"
        className="flex border-b border-border"
      >
        {(
          [
            { id: "contacts", label: "People" },
            { id: "messages", label: "Messages" },
          ] as const
        ).map((tab) => (
          <button
            key={tab.id}
            type="button"
            role="tab"
            aria-selected={mode === tab.id}
            onClick={() => setMode(tab.id)}
            className={`border-b-2 px-3 py-1.5 text-[13px] ${
              mode === tab.id
                ? "border-accent text-text"
                : "border-transparent text-muted hover:text-text"
            }`}
          >
            {tab.label}
          </button>
        ))}
      </div>
      <p className="mt-2 mb-3 text-[12px] text-muted">
        {mode === "contacts"
          ? "Find people by name, number, or activity."
          : "Find messages by what they say."}
      </p>
      <div className="space-y-2">
        {mode === "contacts" ? (
          <>
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
            <Field label="Handle">
              <input
                className={inputClass}
                value={handle}
                onChange={(e) => setHandle(e.target.value)}
                placeholder="Name or number"
              />
            </Field>
            <DateRangeField
              label="First Contact"
              value={firstContact}
              onChange={setFirstContact}
            />
            <DateRangeField
              label="Last Contact"
              value={lastContact}
              onChange={setLastContact}
            />
            <CountField
              label="Group messages"
              value={groupCount}
              onChange={setGroupCount}
            />
            <CountField
              label="Message count"
              value={messageCount}
              onChange={setMessageCount}
            />
          </>
        ) : (
          <>
            <Field label="Within">
              <select
                className={inputClass}
                value={within}
                onChange={(e) => setWithin(e.target.value)}
              >
                <option value="">All messages</option>
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
            <label className="flex items-center gap-2 pl-32 text-[13px] text-text">
              <input
                type="checkbox"
                checked={hasAttachment}
                onChange={(e) => setHasAttachment(e.target.checked)}
                className="size-3.5 accent-accent"
              />
              Has an attachment
            </label>
          </>
        )}
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

function CountField({
  label,
  value,
  onChange,
}: {
  label: string;
  value: CountFilterInput;
  onChange: (next: CountFilterInput) => void;
}) {
  return (
    <Field label={label}>
      <div className="grid min-w-0 grid-cols-[7rem_minmax(0,1fr)] gap-1.5">
        <select
          className={`${inputClass} min-w-0`}
          value={value.comparator}
          aria-label={`${label} comparison`}
          onChange={(e) =>
            onChange({
              ...value,
              comparator: e.target.value as CountComparator | "any",
            })
          }
        >
          <option value="any">Any</option>
          <option value="=">Equal to</option>
          <option value=">">More than</option>
          <option value="<">Less than</option>
        </select>
        <input
          type="number"
          min="0"
          step="1"
          className={`${inputClass} min-w-0`}
          value={value.value ?? ""}
          disabled={value.comparator === "any"}
          aria-label={`${label} value`}
          onChange={(e) => onChange({ ...value, value: e.target.value })}
        />
      </div>
    </Field>
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

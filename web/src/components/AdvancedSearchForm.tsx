"use client";

import {
  composeSearchQuery,
  type AdvancedSearchForm as FormState,
} from "@/lib/searchQuery";
import { useEffect, useState, type ReactNode } from "react";

const inputClass =
  "w-full rounded-md border border-border bg-elevated px-2.5 py-1.5 text-[13px] text-text outline-none placeholder:text-muted focus:border-accent";
const labelClass = "w-28 shrink-0 text-[13px] text-muted";

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
  const [withPerson, setWithPerson] = useState("");
  const [hasWords, setHasWords] = useState("");
  const [doesntHave, setDoesntHave] = useState("");
  const [after, setAfter] = useState("");
  const [before, setBefore] = useState("");
  const [lastContact, setLastContact] = useState("");
  const [firstContact, setFirstContact] = useState("");
  const [source, setSource] = useState("");
  const [conversationType, setConversationType] = useState<
    "any" | "group" | "individual"
  >("any");
  const [hasAttachment, setHasAttachment] = useState(false);
  const [label, setLabel] = useState("");
  const [includeTrash, setIncludeTrash] = useState(false);

  useEffect(() => {
    // Seed free-text into Has the words when opening with a plain query.
    if (initialQuery.trim() && !/[a-z]+:/i.test(initialQuery)) {
      setHasWords(initialQuery.trim());
    }
  }, [initialQuery]);

  const submit = () => {
    const form: FormState = {
      withPerson,
      hasWords,
      doesntHave,
      after,
      before,
      lastContact,
      firstContact,
      source: source || undefined,
      conversationType,
      hasAttachment,
      label: label || undefined,
      includeTrash,
    };
    onSearch(composeSearchQuery(form));
  };

  return (
    <div className="border-b border-border bg-panel px-3 py-3">
      <div className="space-y-2">
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
        <Field label="After">
          <input
            type="date"
            className={inputClass}
            value={after}
            onChange={(e) => setAfter(e.target.value)}
          />
        </Field>
        <Field label="Before">
          <input
            type="date"
            className={inputClass}
            value={before}
            onChange={(e) => setBefore(e.target.value)}
          />
        </Field>
        <Field label="Last contact">
          <input
            className={inputClass}
            value={lastContact}
            onChange={(e) => setLastContact(e.target.value)}
            placeholder="e.g. 1y — no messages for at least this long"
            spellCheck={false}
          />
        </Field>
        <Field label="First contact">
          <input
            className={inputClass}
            value={firstContact}
            onChange={(e) => setFirstContact(e.target.value)}
            placeholder="e.g. 5y — first message at least this long ago"
            spellCheck={false}
          />
        </Field>
        <Field label="In">
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
            <option value="group">Groups only</option>
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
        {labels.length > 0 ? (
          <Field label="Label">
            <select
              className={inputClass}
              value={label}
              onChange={(e) => setLabel(e.target.value)}
            >
              <option value="">Any label</option>
              {labels.map((name) => (
                <option key={name} value={name}>
                  {name}
                </option>
              ))}
            </select>
          </Field>
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
        <label className="flex items-center gap-2 pl-32 text-[13px] text-text">
          <input
            type="checkbox"
            checked={includeTrash}
            onChange={(e) => setIncludeTrash(e.target.checked)}
            className="size-3.5 accent-accent"
          />
          Include Trash
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

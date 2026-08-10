import { useEffect, useRef, useState, type Key } from "react";
import {
  ComboBox,
  Input,
  ListBox,
  ListBoxItem,
  Popover,
} from "react-aria-components";
import { apiClient } from "../lib/api";

/** Operators the conversation list API actually understands. */
const OPERATORS = ["handle:", "contact:", "is:", "participants:"];

interface ContactName {
  id: string;
  name: string;
}

/** One autocomplete entry: unique id, displayed label, text inserted into the query. */
interface Suggestion {
  id: string;
  label: string;
  insert: string;
}

export default function GlobalSearch({
  value,
  onChange,
  onSubmit,
  mode = "search",
}: {
  value: string;
  onChange: (v: string) => void;
  onSubmit: (q: string) => void;
  /** `filter` = live contact list filter (no vault operators). */
  mode?: "search" | "filter";
}) {
  const isFilter = mode === "filter";
  const [contacts, setContacts] = useState<ContactName[]>([]);
  // True between React Aria selecting a suggestion and the end of the keydown
  // that did it. Lets the Enter handler tell "selected a suggestion" apart from
  // "submit the query". Reset on every keydown (capture phase) so a selection
  // made by an earlier event (e.g. a click) never suppresses a later Enter.
  const selectedRef = useRef(false);

  const lastToken = value.split(/\s+/).pop() || "";
  const colonIdx = lastToken.indexOf(":");
  const completingValue = colonIdx !== -1;
  const opPrefix = completingValue ? lastToken.slice(0, colonIdx + 1) : "";
  const opLower = opPrefix.toLowerCase();
  const valuePart = completingValue
    ? lastToken.slice(colonIdx + 1).replace(/^"|"$/g, "")
    : "";

  useEffect(() => {
    // Only contact-complete for handle:/contact: — not participants:/is:.
    const contactOps = opLower === "handle:" || opLower === "contact:";
    if (isFilter || !completingValue || !contactOps) {
      setContacts([]);
      return;
    }
    const ac = new AbortController();
    const t = window.setTimeout(() => {
      const params = new URLSearchParams({
        q: valuePart,
        limit: "20",
        offset: "0",
      });
      apiClient
        .get<{ contacts: ContactName[] }>(`/v1/export/contacts?${params}`, {
          signal: ac.signal,
        })
        .then((res) =>
          setContacts(
            (res.contacts || []).map((c) => ({
              ...c,
              id: String(c.id),
            })),
          ),
        )
        .catch(() => {
          if (!ac.signal.aborted) setContacts([]);
        });
    }, 150);
    return () => {
      window.clearTimeout(t);
      ac.abort();
    };
  }, [isFilter, completingValue, valuePart, opLower]);

  // Empty lastToken must not match every operator via startsWith("") — that made Enter
  // insert an operator after a trailing space instead of running the search.
  const contactOps = opLower === "handle:" || opLower === "contact:";
  const suggestions: Suggestion[] = isFilter
    ? []
    : completingValue && contactOps
      ? contacts
          .slice(0, 6)
          .map((c) => ({
            id: c.id,
            label: c.name,
            // Prefer contact:<id> so names with spaces do not break token parsing.
            insert: `contact:${c.id}`,
          }))
      : !completingValue && lastToken.length > 0
        ? OPERATORS.filter((op) => op.startsWith(lastToken.toLowerCase())).map(
            (op) => ({ id: op, label: op, insert: `${op} ` }),
          )
        : [];

  const applySuggestion = (s: Suggestion) => {
    const tokens = value.split(/\s+/);
    tokens.pop();
    onChange(tokens.concat(s.insert).join(" "));
  };

  const handleSelection = (key: Key | null) => {
    // Null = Escape / blur / Enter without a highlighted item — nothing to insert.
    if (key == null) return;
    selectedRef.current = true;
    const s = suggestions.find((s) => s.id === key);
    if (s) applySuggestion(s);
  };

  return (
    <ComboBox
      allowsCustomValue
      defaultFilter={() => true} // Server-side filtering only.
      inputValue={value}
      onInputChange={onChange}
      selectedKey={null}
      onSelectionChange={handleSelection}
      onKeyDown={(e) => {
        if (e.key !== "Enter") return;
        e.preventDefault();
        // Enter selected a suggestion — React Aria already applied it above,
        // so submitting the raw query here would double-apply.
        if (selectedRef.current) return;
        onSubmit(value);
      }}
      className="relative"
    >
      <div className="flex items-center rounded-xl border border-border bg-bg focus-within:border-accent">
        <Input
          type="search"
          onKeyDownCapture={() => {
            selectedRef.current = false;
          }}
          placeholder={
            isFilter
              ? "Filter by name or handle…"
              : "Filter conversations — name, handle:, is:group, participants:=5"
          }
          className="flex-1 border-none bg-transparent px-3 py-2.5 text-[0.875rem] text-text outline-none"
        />
      </div>
      <Popover className="z-[100] min-w-[var(--trigger-width)] rounded-md border border-border bg-popover p-1 shadow-md outline-none">
        <ListBox className="max-h-72 overflow-auto outline-none">
          {suggestions.map((s) => (
            <ListBoxItem
              key={s.id}
              id={s.id}
              textValue={s.label}
              className={({ isFocused }) =>
                `cursor-pointer rounded px-2 py-1 text-[0.875rem] ${isFocused ? "bg-hover" : ""} text-text`
              }
            >
              {s.label}
            </ListBoxItem>
          ))}
        </ListBox>
      </Popover>
    </ComboBox>
  );
}

import { type Key, useEffect, useRef, useState } from "react";
import { ComboBox, Input, ListBox, ListBoxItem, Popover } from "react-aria-components";
import { apiClient } from "../lib/api";
import { popupShadow } from "../lib/uiStyles";
import { Z_POPOVER } from "../lib/zLayers";

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

/** Autocomplete rows for the conversation search box. */
function buildSearchSuggestions(args: {
  isFilter: boolean;
  completingValue: boolean;
  contactOps: boolean;
  lastToken: string;
  contacts: ContactName[];
}): Suggestion[] {
  if (args.isFilter) return [];
  if (args.completingValue && args.contactOps) {
    return args.contacts.slice(0, 6).map((c) => ({
      id: c.id,
      label: c.name,
      // Use contact:<id> so names with spaces do not break the query.
      insert: `contact:${c.id}`,
    }));
  }
  if (!args.completingValue && args.lastToken.length > 0) {
    return OPERATORS.filter((op) => op.startsWith(args.lastToken.toLowerCase())).map((op) => ({
      id: op,
      label: op,
      insert: `${op} `,
    }));
  }
  return [];
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
  /** `filter` = live contact list filter (no vault search operators). */
  mode?: "search" | "filter";
}) {
  const isFilter = mode === "filter";
  const [contacts, setContacts] = useState<ContactName[]>([]);
  // True from the moment a suggestion is chosen until the next keydown.
  // Lets Enter tell "chose a suggestion" apart from "run the search".
  const selectedRef = useRef(false);

  const lastToken = value.split(/\s+/).pop() || "";
  const colonIdx = lastToken.indexOf(":");
  const completingValue = colonIdx !== -1;
  const opPrefix = completingValue ? lastToken.slice(0, colonIdx + 1) : "";
  const opLower = opPrefix.toLowerCase();
  const valuePart = completingValue ? lastToken.slice(colonIdx + 1).replace(/^"|"$/g, "") : "";

  useEffect(() => {
    // Suggest contact names only for handle: and contact:, not for is: or participants:.
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

  // An empty last token must not match every operator. That used to insert
  // an operator after a trailing space instead of running the search.
  const contactOps = opLower === "handle:" || opLower === "contact:";
  const suggestions = buildSearchSuggestions({
    isFilter,
    completingValue,
    contactOps,
    lastToken,
    contacts,
  });

  const applySuggestion = (s: Suggestion) => {
    const tokens = value.split(/\s+/);
    tokens.pop();
    onChange(tokens.concat(s.insert).join(" "));
  };

  const handleSelection = (key: Key | null) => {
    // Escape, blur, or Enter with nothing highlighted: do not insert text.
    if (key == null) return;
    selectedRef.current = true;
    const s = suggestions.find((s) => s.id === key);
    if (s) applySuggestion(s);
  };

  return (
    <ComboBox
      allowsCustomValue
      defaultFilter={() => true} // Filtering happens on the server.
      inputValue={value}
      onInputChange={onChange}
      selectedKey={null}
      onSelectionChange={handleSelection}
      onKeyDown={(e) => {
        if (e.key !== "Enter") return;
        e.preventDefault();
        // Enter already applied a suggestion. Submitting the raw query would apply it twice.
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
      <Popover
        className={`box-border w-[var(--trigger-width)] max-w-[var(--trigger-width)] rounded-md border border-border bg-popover p-1 outline-none ${Z_POPOVER} ${popupShadow}`}
      >
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

import { useEffect, useState } from "react";
import { apiClient } from "./api";

/** Operators the conversation list API actually understands. */
const OPERATORS = ["handle:", "contact:", "is:", "participants:"];

interface ContactName {
  id: string;
  name: string;
}

/** One autocomplete entry: unique id, displayed label, text inserted into the query. */
export interface Suggestion {
  id: string;
  label: string;
  insert: string;
}

/** Autocomplete rows for a conversation search box. */
export function buildSearchSuggestions(args: {
  completingValue: boolean;
  contactOps: boolean;
  lastToken: string;
  contacts: ContactName[];
}): Suggestion[] {
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

/** Replace the token being typed with a suggestion's text. */
export function applySuggestionToQuery(value: string, suggestion: Suggestion): string {
  const tokens = value.split(/\s+/);
  tokens.pop();
  return tokens.concat(suggestion.insert).join(" ");
}

/**
 * Operator autocomplete for the conversation-style search bars. `handle:` and
 * `contact:` fetch matching contact names from the vault; a bare prefix
 * completes to the operator itself. Disabled bars (contacts search) get nothing.
 */
export function useSearchSuggestions(value: string, enabled: boolean): Suggestion[] {
  const [contacts, setContacts] = useState<ContactName[]>([]);

  const lastToken = value.split(/\s+/).pop() || "";
  const colonIdx = lastToken.indexOf(":");
  const completingValue = colonIdx !== -1;
  const opLower = completingValue ? lastToken.slice(0, colonIdx + 1).toLowerCase() : "";
  const valuePart = completingValue ? lastToken.slice(colonIdx + 1).replace(/^"|"$/g, "") : "";
  // Suggest contact names only for handle: and contact:, not for is: or participants:.
  const contactOps = opLower === "handle:" || opLower === "contact:";

  useEffect(() => {
    if (!enabled || !completingValue || !contactOps) {
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
  }, [enabled, completingValue, contactOps, valuePart]);

  if (!enabled) return [];
  // An empty last token must not match every operator. That would insert an
  // operator after a trailing space instead of running the search.
  return buildSearchSuggestions({ completingValue, contactOps, lastToken, contacts });
}

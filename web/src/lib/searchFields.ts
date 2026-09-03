/**
 * The search language, as the vault describes it.
 *
 * The browser keeps no list of words of its own: `GET /v1/search/fields` says
 * which words one list accepts, what kind of value each takes, and which values
 * a choice word allows, and the search box builds its suggestions from that.
 * What is left here are two rules about the *shape* of a query — whether it
 * carries a `word:` token at all, and what its plain words are — which the
 * contact and conversation lists use to decide whether they can narrow rows in
 * the browser or must ask the vault.
 */

import { listSearchFields } from "./vaultApi";
import type { components } from "./vaultApi.types";
import { keys } from "./vaultKeys";
import { useVaultQuery } from "./vaultQuery";

type Schema = components["schemas"];
export type SearchField = Schema["FieldDoc"];
export type SearchList = Schema["ListKind"];

/**
 * A `word:` token: a field name followed by a colon, at the start or after a
 * space or an opening bracket, with or without a leading minus. Quoted phrases
 * are removed first so a colon inside one does not count.
 */
// The bracket matters because `(kind:group or kind:direct)` is a real query.
// The value must not start with `/`, so a pasted URL like `http://x` is a word.
const FIELD_TOKEN_RE = /(^|[\s(])-?[a-z][a-z-]*:(?!\/)/i;
const PHRASE_RE = /"(?:[^"]|"")*"/g;

/** True when the query has a `word:` token, which only the vault can apply. */
export function hasFieldToken(q: string): boolean {
  return FIELD_TOKEN_RE.test(q.replace(PHRASE_RE, " "));
}

/** The free-text words of a query, with every `word:value` token removed. */
export function stripFieldTokens(q: string): string {
  return q
    .replace(/(^|[\s(])-?[a-z][a-z-]*:(?!\/)("(?:[^"]|"")*"|\S*)/gi, " ")
    .replace(/\s+/g, " ")
    .trim();
}

/** The words one list accepts, from the vault, cached for the session. */
export function useSearchFields(list: SearchList): { fields: SearchField[]; loading: boolean } {
  const { data, isPending } = useVaultQuery(
    keys.searchFields.list(list),
    async (signal) => (await listSearchFields(list, { signal })).items,
    { staleTime: Number.POSITIVE_INFINITY },
  );
  return { fields: data ?? [], loading: isPending };
}

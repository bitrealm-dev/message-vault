"use client";

import { searchContacts } from "@/lib/contactSearch";
import type { ContactListItem } from "@/lib/types";
import { useCallback, useMemo } from "react";
import type { SortMode, SortOrder } from "./SortByMenu";

/**
 * Contact list pipeline for BrowseShell: visibility filter, sort/search,
 * pin-selected-above-search, and letter grouping.
 *
 * Call `useBrowseContactListBase` before `useListSelection`, then
 * `useBrowseContactListView` after selection is available so pinning stays correct.
 */
export function useBrowseContactListBase(options: {
  contacts: ContactListItem[];
  sort: SortMode;
  sortOrder: SortOrder;
  query: string;
}): {
  visibleContacts: ContactListItem[];
  sortedRaw: ContactListItem[];
  selectAllIds: number[];
  compareContacts: (a: ContactListItem, b: ContactListItem) => number;
} {
  const {
    contacts,
    sort,
    sortOrder,
    query,
  } = options;

  const visibleContacts = contacts;

  const compareContacts = useCallback(
    (a: ContactListItem, b: ContactListItem) => {
      let cmp = 0;
      if (sort === "messages") {
        cmp = a.messageCount - b.messageCount;
        if (cmp === 0) {
          cmp =
            a.sortLast.localeCompare(b.sortLast, undefined, {
              sensitivity: "base",
            }) ||
            a.sortFirst.localeCompare(b.sortFirst, undefined, {
              sensitivity: "base",
            });
        }
      } else if (sort === "group-messages") {
        cmp = a.groupMessageCount - b.groupMessageCount;
        if (cmp === 0) {
          cmp =
            a.sortLast.localeCompare(b.sortLast, undefined, {
              sensitivity: "base",
            }) ||
            a.sortFirst.localeCompare(b.sortFirst, undefined, {
              sensitivity: "base",
            });
        }
      } else if (sort === "phone") {
        const aHandle = a.preferredHandle ?? "";
        const bHandle = b.preferredHandle ?? "";
        const aDigits = aHandle.replace(/\D/g, "");
        const bDigits = bHandle.replace(/\D/g, "");
        if (aDigits && bDigits) {
          cmp = aDigits.localeCompare(bDigits, undefined, { numeric: true });
        } else {
          cmp = aHandle.localeCompare(bHandle, undefined, {
            sensitivity: "base",
          });
        }
        if (cmp === 0) {
          cmp =
            a.sortLast.localeCompare(b.sortLast, undefined, {
              sensitivity: "base",
            }) ||
            a.sortFirst.localeCompare(b.sortFirst, undefined, {
              sensitivity: "base",
            });
        }
      } else if (sort === "first") {
        cmp =
          a.sortFirst.localeCompare(b.sortFirst, undefined, {
            sensitivity: "base",
          }) ||
          a.sortLast.localeCompare(b.sortLast, undefined, {
            sensitivity: "base",
          });
      } else {
        cmp =
          a.sortLast.localeCompare(b.sortLast, undefined, {
            sensitivity: "base",
          }) ||
          a.sortFirst.localeCompare(b.sortFirst, undefined, {
            sensitivity: "base",
          });
      }
      return sortOrder === "desc" ? -cmp : cmp;
    },
    [sort, sortOrder],
  );

  const sortedRaw = useMemo(() => {
    const q = query.trim();
    if (q) {
      return searchContacts(visibleContacts, q);
    }
    const copy = [...visibleContacts];
    copy.sort(compareContacts);
    return copy;
  }, [visibleContacts, compareContacts, query]);

  const selectAllIds = useMemo(
    () => visibleContacts.map((c) => c.id),
    [visibleContacts],
  );

  return { visibleContacts, sortedRaw, selectAllIds, compareContacts };
}

export function useBrowseContactListView(options: {
  sortedRaw: ContactListItem[];
  visibleContacts: ContactListItem[];
  compareContacts: (a: ContactListItem, b: ContactListItem) => number;
  query: string;
  selectedIds: ReadonlySet<number>;
  sort: SortMode;
}): {
  sorted: ContactListItem[];
  grouped: [string, ContactListItem[]][];
} {
  const {
    sortedRaw,
    visibleContacts,
    compareContacts,
    query,
    selectedIds,
    sort,
  } = options;

  const sorted = useMemo(() => {
    const q = query.trim();
    if (!q || selectedIds.size === 0) return sortedRaw;
    const pinned = visibleContacts.filter((c) => selectedIds.has(c.id));
    pinned.sort(compareContacts);
    const pinnedIds = new Set(pinned.map((c) => c.id));
    return [...pinned, ...sortedRaw.filter((c) => !pinnedIds.has(c.id))];
  }, [sortedRaw, query, selectedIds, visibleContacts, compareContacts]);

  const grouped = useMemo(() => {
    // Flat list while searching so pinned checked contacts stay at the top.
    if (
      sort === "messages" ||
      sort === "group-messages" ||
      sort === "phone" ||
      query.trim()
    ) {
      return [["", sorted]] as [string, ContactListItem[]][];
    }
    const map = new Map<string, ContactListItem[]>();
    for (const c of sorted) {
      const letterSrc = sort === "first" ? c.sortFirst : c.sortLast;
      const ch = letterSrc.charAt(0).toUpperCase();
      const letter = ch >= "A" && ch <= "Z" ? ch : "#";
      if (!map.has(letter)) map.set(letter, []);
      map.get(letter)!.push(c);
    }
    return [...map.entries()].sort(([a], [b]) => {
      if (a === "#") return 1;
      if (b === "#") return -1;
      return a.localeCompare(b);
    });
  }, [sorted, sort, query]);

  return { sorted, grouped };
}

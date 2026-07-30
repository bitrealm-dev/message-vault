"use client";

import type { ContactDetail, ContactListItem } from "@/lib/types";
import { useRouter } from "next/navigation";
import {
  useCallback,
  useMemo,
  useRef,
  useState,
  type Dispatch,
  type SetStateAction,
} from "react";
import type { LabelCheckState } from "./LabelsMenu";

export type UseBrowseLabelMembershipOptions = {
  allLabels: string[];
  contacts: ContactListItem[];
  selectedContacts: ContactListItem[];
  hasSelection: boolean;
  detail: ContactDetail | null;
  setDetail: Dispatch<SetStateAction<ContactDetail | null>>;
  contactId: number | null;
  setThreadsEpoch: Dispatch<SetStateAction<number>>;
  formOpen: boolean;
  /**
   * `labelOverrides`/`excludeOverrides` are owned by `BrowseShell` (not this
   * hook) because `isContactExcluded` needs the current exclude map to filter
   * the contact list before this hook's other inputs (e.g. `selectedContacts`)
   * exist. Everything else about assign/clear/exclude membership lives here.
   */
  labelOverrides: Map<number, string[]>;
  setLabelOverrides: Dispatch<SetStateAction<Map<number, string[]>>>;
  excludeOverrides: Map<number, boolean>;
  setExcludeOverrides: Dispatch<SetStateAction<Map<number, boolean>>>;
  ctxMenu: { id: number; x: number; y: number } | null;
  trashIdsForContext: (ctxId: number) => number[];
  queueStatusMessage: (message: string) => void;
};

export type UseBrowseLabelMembershipResult = {
  labelsPanelWrapRef: React.RefObject<HTMLDivElement | null>;
  labelsCreatePinnedRef: React.RefObject<boolean>;
  labelsPanelPos: { x: number; y: number } | null;
  selectionDirtyRef: React.RefObject<boolean>;
  canEditLabels: boolean;
  menuLabels: string[];
  labelChecks: Record<string, LabelCheckState>;
  excludedCheck: LabelCheckState;
  toggleLabel: (name: string) => void;
  createAndAssignLabel: (name: string) => void;
  clearAllLabelsForSelection: () => Promise<void>;
  toggleExcludedForSelection: () => Promise<void>;
  onSelectionMenuOpenChange: (open: boolean) => void;
  openCtxLabels: (anchor: DOMRect) => void;
  closeLabelsPanel: () => void;
  scheduleCloseLabelsPanel: () => void;
  cancelCloseLabelsPanel: () => void;
  flushSelectionDirty: () => void;
};

/** Contact-label assign/clear/exclude membership + the labels flyout panel state. */
export function useBrowseLabelMembership(
  options: UseBrowseLabelMembershipOptions,
): UseBrowseLabelMembershipResult {
  const {
    allLabels,
    contacts,
    selectedContacts,
    hasSelection,
    detail,
    setDetail,
    contactId,
    setThreadsEpoch,
    formOpen,
    labelOverrides,
    setLabelOverrides,
    excludeOverrides,
    setExcludeOverrides,
    ctxMenu,
    trashIdsForContext,
    queueStatusMessage,
  } = options;

  const router = useRouter();

  const labelOverridesRef = useRef(labelOverrides);
  labelOverridesRef.current = labelOverrides;
  const excludeOverridesRef = useRef(excludeOverrides);
  excludeOverridesRef.current = excludeOverrides;

  const labelsPanelWrapRef = useRef<HTMLDivElement>(null);
  const labelsCloseTimerRef = useRef<ReturnType<typeof setTimeout> | null>(
    null,
  );
  /** Keep the labels flyout open while the create form is showing. */
  const labelsCreatePinnedRef = useRef(false);
  const [labelTargetOverrideIds, setLabelTargetOverrideIds] = useState<
    number[] | null
  >(null);
  const [labelsPanelPos, setLabelsPanelPos] = useState<{
    x: number;
    y: number;
  } | null>(null);
  const selectionDirtyRef = useRef(false);

  const closeLabelsPanel = useCallback(() => {
    if (labelsCloseTimerRef.current) {
      clearTimeout(labelsCloseTimerRef.current);
      labelsCloseTimerRef.current = null;
    }
    labelsCreatePinnedRef.current = false;
    setLabelsPanelPos(null);
    setLabelTargetOverrideIds(null);
  }, []);

  const flushSelectionDirty = useCallback(() => {
    if (!selectionDirtyRef.current) return;
    selectionDirtyRef.current = false;
    const labelOv = labelOverridesRef.current;
    const excludeOv = excludeOverridesRef.current;
    // Keep the open contact card in sync — overrides are cleared next, and
    // router.refresh() only updates the list props, not client `detail`.
    setDetail((prev) => {
      if (!prev) return prev;
      const labels = labelOv.get(prev.id);
      const hasExclude = excludeOv.has(prev.id);
      if (!labels && !hasExclude) return prev;
      return {
        ...prev,
        ...(labels ? { labels } : {}),
        ...(hasExclude ? { exclude: excludeOv.get(prev.id)! } : {}),
      };
    });
    setLabelOverrides(new Map());
    setExcludeOverrides(new Map());
    router.refresh();
  }, [router, setDetail, setLabelOverrides, setExcludeOverrides]);

  const cancelCloseLabelsPanel = useCallback(() => {
    if (labelsCloseTimerRef.current) {
      clearTimeout(labelsCloseTimerRef.current);
      labelsCloseTimerRef.current = null;
    }
  }, []);

  const scheduleCloseLabelsPanel = useCallback(() => {
    if (labelsCreatePinnedRef.current) return;
    cancelCloseLabelsPanel();
    labelsCloseTimerRef.current = setTimeout(() => {
      labelsCloseTimerRef.current = null;
      setLabelsPanelPos(null);
      setLabelTargetOverrideIds(null);
    }, 400);
  }, [cancelCloseLabelsPanel]);

  const openCtxLabels = useCallback(
    (anchor: DOMRect) => {
      if (!ctxMenu || formOpen) return;
      const ids = trashIdsForContext(ctxMenu.id);
      if (ids.length === 0) return;
      cancelCloseLabelsPanel();
      const x = Math.max(
        8,
        Math.min(anchor.right - 4, window.innerWidth - 272),
      );
      const y = Math.max(8, Math.min(anchor.top, window.innerHeight - 320));
      setLabelTargetOverrideIds(ids);
      setLabelsPanelPos({ x, y });
    },
    [ctxMenu, formOpen, trashIdsForContext, cancelCloseLabelsPanel],
  );

  const labelsFor = useCallback(
    (id: number, fallback: string[]) => labelOverrides.get(id) ?? fallback,
    [labelOverrides],
  );

  const labelTargets = useMemo(() => {
    if (labelTargetOverrideIds?.length) {
      return labelTargetOverrideIds.flatMap((id) => {
        const c =
          contacts.find((x) => x.id === id) ??
          selectedContacts.find((x) => x.id === id) ??
          (detail?.id === id ? detail : null);
        if (!c) return [];
        return [
          {
            id: c.id,
            labels: labelsFor(c.id, c.labels),
          },
        ];
      });
    }
    if (hasSelection) {
      return selectedContacts.map((c) => ({
        id: c.id,
        labels: labelsFor(c.id, c.labels),
      }));
    }
    if (detail) {
      return [{ id: detail.id, labels: labelsFor(detail.id, detail.labels) }];
    }
    return [] as Array<{ id: number; labels: string[] }>;
  }, [
    labelTargetOverrideIds,
    contacts,
    hasSelection,
    selectedContacts,
    detail,
    labelsFor,
  ]);

  const menuLabels = useMemo(() => {
    const names = new Set(allLabels);
    for (const person of labelTargets) {
      for (const label of person.labels) names.add(label);
    }
    return [...names].sort((a, b) =>
      a.localeCompare(b, undefined, { sensitivity: "base" }),
    );
  }, [allLabels, labelTargets]);

  const labelChecks = useMemo(() => {
    const result: Record<string, LabelCheckState> = {};
    const n = labelTargets.length;
    for (const name of menuLabels) {
      if (n === 0) {
        result[name] = "off";
        continue;
      }
      let count = 0;
      for (const person of labelTargets) {
        if (person.labels.includes(name)) count++;
      }
      result[name] = count === 0 ? "off" : count === n ? "on" : "mixed";
    }
    return result;
  }, [menuLabels, labelTargets]);

  const applyLabelMembership = useCallback(
    async (name: string, enable: boolean) => {
      const targets = labelTargets;
      if (targets.length === 0) return;

      let changed = 0;
      for (const person of targets) {
        if (person.labels.includes(name) !== enable) changed++;
      }
      if (changed === 0) return;

      const nextLabelsById = new Map<number, string[]>();
      for (const person of targets) {
        const current =
          labelOverridesRef.current.get(person.id) ?? person.labels;
        const has = current.includes(name);
        if (enable === has) {
          nextLabelsById.set(person.id, current);
          continue;
        }
        const labels = enable
          ? [...current, name].sort((a, b) =>
              a.localeCompare(b, undefined, { sensitivity: "base" }),
            )
          : current.filter((l) => l !== name);
        nextLabelsById.set(person.id, labels);
      }

      // Optimistic UI so the menu can stay open across multiple toggles.
      setLabelOverrides((prev) => {
        const next = new Map(prev);
        for (const [id, labels] of nextLabelsById) {
          next.set(id, labels);
        }
        return next;
      });
      // Contact card reads `detail` after overrides flush — update it now.
      setDetail((prev) => {
        if (!prev) return prev;
        const labels = nextLabelsById.get(prev.id);
        if (!labels) return prev;
        return { ...prev, labels };
      });
      selectionDirtyRef.current = true;

      const noun = changed === 1 ? "contact" : "contacts";
      queueStatusMessage(
        enable
          ? `Added ${changed} ${noun} to ${name}`
          : `Removed ${changed} ${noun} from ${name}`,
      );

      try {
        const ids = targets
          .filter((person) => person.labels.includes(name) !== enable)
          .map((person) => person.id);
        const res = await fetch("/api/contacts/labels", {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ ids, name, enable }),
        });
        const data = await res.json();
        if (!res.ok) throw new Error(data.error ?? "save failed");
      } catch (err) {
        console.error(err);
        // Re-sync from server on failure.
        selectionDirtyRef.current = true;
        router.refresh();
        setLabelOverrides(new Map());
        setThreadsEpoch((n) => n + 1);
      }
    },
    [
      labelTargets,
      router,
      queueStatusMessage,
      setLabelOverrides,
      setDetail,
      setThreadsEpoch,
    ],
  );

  const toggleLabel = useCallback(
    (name: string) => {
      const state = labelChecks[name] ?? "off";
      const enable = state !== "on";
      void applyLabelMembership(name, enable);
    },
    [labelChecks, applyLabelMembership],
  );

  const createAndAssignLabel = useCallback(
    (name: string) => {
      void (async () => {
        await applyLabelMembership(name, true);
        // Fixed context-menu flyout unmounts without onOpenChange(false), so
        // refresh here so the left Labels nav picks up the new name.
        router.refresh();
      })();
    },
    [applyLabelMembership, router],
  );

  const onSelectionMenuOpenChange = useCallback(
    (open: boolean) => {
      if (open) return;
      flushSelectionDirty();
    },
    [flushSelectionDirty],
  );

  const selectionFieldTargets = useMemo(() => {
    if (labelTargetOverrideIds?.length) {
      return labelTargetOverrideIds.flatMap((id) => {
        const c =
          contacts.find((x) => x.id === id) ??
          selectedContacts.find((x) => x.id === id) ??
          (detail?.id === id ? detail : null);
        if (!c) return [];
        return [
          {
            id: c.id,
            exclude: excludeOverrides.get(c.id) ?? c.exclude,
          },
        ];
      });
    }
    if (hasSelection) {
      return selectedContacts.map((c) => ({
        id: c.id,
        exclude: excludeOverrides.get(c.id) ?? c.exclude,
      }));
    }
    if (detail) {
      return [
        {
          id: detail.id,
          exclude: excludeOverrides.get(detail.id) ?? detail.exclude,
        },
      ];
    }
    return [] as Array<{ id: number; exclude: boolean }>;
  }, [
    labelTargetOverrideIds,
    contacts,
    hasSelection,
    selectedContacts,
    detail,
    excludeOverrides,
  ]);

  const excludedCheck = useMemo((): LabelCheckState => {
    const n = selectionFieldTargets.length;
    if (n === 0) return "off";
    let excluded = 0;
    for (const p of selectionFieldTargets) {
      if (p.exclude) excluded++;
    }
    if (excluded === 0) return "off";
    if (excluded === n) return "on";
    return "mixed";
  }, [selectionFieldTargets]);

  const patchContactFields = useCallback(
    async (id: number, patch: { exclude?: boolean }) => {
      const res = await fetch(`/api/contacts/${id}`, {
        method: "PATCH",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(patch),
      });
      const data = await res.json();
      if (!res.ok) throw new Error(data.error ?? "save failed");
      if (data.contact && id === contactId) setDetail(data.contact);
    },
    [contactId, setDetail],
  );

  const toggleExcludedForSelection = useCallback(async () => {
    const targets = selectionFieldTargets;
    if (targets.length === 0) return;
    const excludeAll = excludedCheck !== "on";
    let changed = 0;
    for (const p of targets) {
      if (p.exclude !== excludeAll) changed++;
    }
    if (changed === 0) return;

    setExcludeOverrides((prev) => {
      const next = new Map(prev);
      for (const p of targets) {
        next.set(p.id, excludeAll);
      }
      return next;
    });
    selectionDirtyRef.current = true;

    const noun = changed === 1 ? "contact" : "contacts";
    queueStatusMessage(
      excludeAll
        ? `Made ${changed} ${noun} inactive`
        : `Made ${changed} ${noun} active`,
    );

    try {
      for (const p of targets) {
        if (p.exclude === excludeAll) continue;
        await patchContactFields(p.id, { exclude: excludeAll });
      }
    } catch (err) {
      console.error(err);
      selectionDirtyRef.current = true;
      router.refresh();
      setExcludeOverrides(new Map());
    }
  }, [
    selectionFieldTargets,
    excludedCheck,
    queueStatusMessage,
    patchContactFields,
    router,
    setExcludeOverrides,
  ]);

  const clearAllLabelsForSelection = useCallback(async () => {
    const targets = labelTargets;
    if (targets.length === 0) return;

    const nextLabelsById = new Map<number, string[]>();
    for (const person of targets) {
      nextLabelsById.set(person.id, []);
    }

    setLabelOverrides((prev) => {
      const next = new Map(prev);
      for (const [id, labels] of nextLabelsById) {
        next.set(id, labels);
      }
      return next;
    });
    setDetail((prev) => {
      if (!prev) return prev;
      if (!nextLabelsById.has(prev.id)) return prev;
      return { ...prev, labels: [], exclude: false };
    });

    const excludeTargets = selectionFieldTargets.filter((p) => p.exclude);
    if (excludeTargets.length > 0) {
      setExcludeOverrides((prev) => {
        const next = new Map(prev);
        for (const p of excludeTargets) {
          next.set(p.id, false);
        }
        return next;
      });
    }

    selectionDirtyRef.current = true;
    const noun = targets.length === 1 ? "contact" : "contacts";
    queueStatusMessage(`Cleared labels for ${targets.length} ${noun}`);

    try {
      for (const person of targets) {
        const body: { labels: string[]; exclude?: boolean } = {
          labels: [],
        };
        const wasExcluded =
          excludeOverrides.get(person.id) ??
          selectionFieldTargets.find((p) => p.id === person.id)?.exclude;
        if (wasExcluded) body.exclude = false;

        const res = await fetch(`/api/contacts/${person.id}`, {
          method: "PATCH",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify(body),
        });
        const data = await res.json();
        if (!res.ok) throw new Error(data.error ?? "save failed");
        if (data.contact) {
          setDetail((prev) =>
            prev && prev.id === data.contact.id ? data.contact : prev,
          );
        }
      }
    } catch (err) {
      console.error(err);
      selectionDirtyRef.current = true;
      router.refresh();
      setLabelOverrides(new Map());
      setExcludeOverrides(new Map());
      setThreadsEpoch((n) => n + 1);
    }
  }, [
    labelTargets,
    selectionFieldTargets,
    excludeOverrides,
    queueStatusMessage,
    router,
    setLabelOverrides,
    setExcludeOverrides,
    setDetail,
    setThreadsEpoch,
  ]);

  const canEditLabels = !formOpen && (hasSelection || !!detail);

  return {
    labelsPanelWrapRef,
    labelsCreatePinnedRef,
    labelsPanelPos,
    selectionDirtyRef,
    canEditLabels,
    menuLabels,
    labelChecks,
    excludedCheck,
    toggleLabel,
    createAndAssignLabel,
    clearAllLabelsForSelection,
    toggleExcludedForSelection,
    onSelectionMenuOpenChange,
    openCtxLabels,
    closeLabelsPanel,
    scheduleCloseLabelsPanel,
    cancelCloseLabelsPanel,
    flushSelectionDirty,
  };
}

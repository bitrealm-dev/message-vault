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
  setThreadsEpoch: Dispatch<SetStateAction<number>>;
  formOpen: boolean;
  labelOverrides: Map<number, string[]>;
  setLabelOverrides: Dispatch<SetStateAction<Map<number, string[]>>>;
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
  toggleLabel: (name: string) => void;
  createAndAssignLabel: (name: string) => void;
  clearAllLabelsForSelection: () => Promise<void>;
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
    setThreadsEpoch,
    formOpen,
    labelOverrides,
    setLabelOverrides,
    ctxMenu,
    trashIdsForContext,
    queueStatusMessage,
  } = options;

  const router = useRouter();

  const labelOverridesRef = useRef(labelOverrides);
  labelOverridesRef.current = labelOverrides;

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
    // Keep the open contact card in sync — overrides are cleared next, and
    // router.refresh() only updates the list props, not client `detail`.
    setDetail((prev) => {
      if (!prev) return prev;
      const labels = labelOv.get(prev.id);
      if (!labels) return prev;
      return {
        ...prev,
        labels,
      };
    });
    setLabelOverrides(new Map());
    router.refresh();
  }, [router, setDetail, setLabelOverrides]);

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
      return { ...prev, labels: [] };
    });

    selectionDirtyRef.current = true;
    const noun = targets.length === 1 ? "contact" : "contacts";
    queueStatusMessage(`Cleared labels for ${targets.length} ${noun}`);

    try {
      for (const person of targets) {
        const res = await fetch(`/api/contacts/${person.id}`, {
          method: "PATCH",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ labels: [] }),
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
      setThreadsEpoch((n) => n + 1);
    }
  }, [
    labelTargets,
    queueStatusMessage,
    router,
    setLabelOverrides,
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
    toggleLabel,
    createAndAssignLabel,
    clearAllLabelsForSelection,
    onSelectionMenuOpenChange,
    openCtxLabels,
    closeLabelsPanel,
    scheduleCloseLabelsPanel,
    cancelCloseLabelsPanel,
    flushSelectionDirty,
  };
}

"use client";

import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type Dispatch,
  type MouseEvent,
  type RefObject,
  type SetStateAction,
} from "react";

export type ListSelectionRangeMode = "anchor" | "selectionSpan";
export type ListSelectionMultiThreshold = "any" | "moreThanOne";
export type ListSelectionRowClickMode =
  | "openWhenEmptyElseToggle"
  | "alwaysOpen"
  | "openWhenEmptyElseToggleIfSelected";
export type ListSelectionCheckboxEvents = "preventAndStop" | "stopOnly";

export type UseListSelectionOptions<TId> = {
  /** Ordered ids used for shift-range indexing. */
  orderedIds: readonly TId[];
  /** Ids used by select-all (defaults to orderedIds). */
  selectAllIds?: readonly TId[];
  /** Valid ids; selection is pruned when this set changes. */
  validIds?: readonly TId[];
  rangeMode: ListSelectionRangeMode;
  /**
   * After an anchor-mode range select, move the anchor to the click
   * (Groups). Unassigned leaves the anchor where it was.
   */
  rangeUpdatesAnchor?: boolean;
  multiThreshold?: ListSelectionMultiThreshold;
  /** Focused row used to seed ctrl/cmd toggle when selection is empty. */
  focusedId?: TId | null;
  /**
   * When seeding an empty selection on ctrl-click, skip adding focusedId
   * if it equals the clicked id (Browse/Unassigned). Groups seeds anyway.
   */
  ctrlSeedSkipsTarget?: boolean;
  rowClickMode: ListSelectionRowClickMode;
  checkboxEvents?: ListSelectionCheckboxEvents;
  /** Clear selection with Escape when any ids are selected. */
  escapeToClear?: boolean;
  /** Return true to skip Escape clear (e.g. open context menu). */
  escapeBlocked?: () => boolean;
  escapePreventDefault?: boolean;
  /** When select-all selects everything, set anchor to first id (Unassigned). */
  selectAllSetsAnchor?: boolean;
  /** Called when opening a row (plain click with no selection action). */
  onOpen?: (id: TId) => void;
  /** Side effects after a selection mutation (not on open). */
  onSelectionMutation?: () => void;
};

export type UseListSelectionResult<TId> = {
  selectedIds: Set<TId>;
  setSelectedIds: Dispatch<SetStateAction<Set<TId>>>;
  selectionAnchorRef: RefObject<TId | null>;
  hasSelection: boolean;
  multiSelected: boolean;
  allSelected: boolean;
  someSelected: boolean;
  selectAllRef: RefObject<HTMLInputElement | null>;
  clearSelection: () => void;
  toggleSelectAll: () => void;
  onSelectColumnClick: (id: TId, e: MouseEvent) => void;
  onRowClick: (
    id: TId,
    e: MouseEvent | { shiftKey: boolean; metaKey?: boolean; ctrlKey?: boolean },
  ) => void;
};

export function useListSelection<TId>(
  options: UseListSelectionOptions<TId>,
): UseListSelectionResult<TId> {
  const {
    orderedIds,
    selectAllIds: selectAllIdsOpt,
    validIds,
    rangeMode,
    rangeUpdatesAnchor = false,
    multiThreshold = "any",
    focusedId = null,
    ctrlSeedSkipsTarget = true,
    rowClickMode,
    checkboxEvents = "preventAndStop",
    escapeToClear = true,
    escapeBlocked,
    escapePreventDefault = false,
    selectAllSetsAnchor = false,
    onOpen,
    onSelectionMutation,
  } = options;

  const selectAllIds = selectAllIdsOpt ?? orderedIds;

  const [selectedIds, setSelectedIds] = useState<Set<TId>>(() => new Set());
  const selectionAnchorRef = useRef<TId | null>(null);
  const selectAllRef = useRef<HTMLInputElement | null>(null);

  const onOpenRef = useRef(onOpen);
  onOpenRef.current = onOpen;
  const onMutationRef = useRef(onSelectionMutation);
  onMutationRef.current = onSelectionMutation;
  const escapeBlockedRef = useRef(escapeBlocked);
  escapeBlockedRef.current = escapeBlocked;
  const focusedIdRef = useRef(focusedId);
  focusedIdRef.current = focusedId;
  const ctrlSeedSkipsTargetRef = useRef(ctrlSeedSkipsTarget);
  ctrlSeedSkipsTargetRef.current = ctrlSeedSkipsTarget;

  const indexById = useMemo(() => {
    const map = new Map<TId, number>();
    orderedIds.forEach((id, i) => map.set(id, i));
    return map;
  }, [orderedIds]);

  const clearSelection = useCallback(() => {
    setSelectedIds(new Set());
    selectionAnchorRef.current = null;
  }, []);

  useEffect(() => {
    if (!validIds) return;
    const valid = new Set(validIds);
    setSelectedIds((prev) => {
      if (prev.size === 0) return prev;
      let changed = false;
      const next = new Set<TId>();
      for (const id of prev) {
        if (valid.has(id)) next.add(id);
        else changed = true;
      }
      return changed ? next : prev;
    });
  }, [validIds]);

  const hasSelection = selectedIds.size > 0;
  const multiSelected =
    multiThreshold === "moreThanOne"
      ? selectedIds.size > 1
      : selectedIds.size >= 1;

  const allSelected = useMemo(() => {
    if (selectAllIds.length === 0) return false;
    return selectAllIds.every((id) => selectedIds.has(id));
  }, [selectAllIds, selectedIds]);

  const someSelected = useMemo(() => {
    if (selectAllIds.length === 0) return false;
    return selectAllIds.some((id) => selectedIds.has(id));
  }, [selectAllIds, selectedIds]);

  useEffect(() => {
    if (!selectAllRef.current) return;
    selectAllRef.current.indeterminate = someSelected && !allSelected;
  }, [someSelected, allSelected]);

  const notifyMutation = useCallback(() => {
    onMutationRef.current?.();
  }, []);

  const applyRangeSelect = useCallback(
    (id: TId) => {
      const clickIndex = indexById.get(id);
      if (clickIndex === undefined) return;

      if (rangeMode === "selectionSpan") {
        const selectedIndices: number[] = [];
        for (const sid of selectedIds) {
          const idx = indexById.get(sid);
          if (idx !== undefined) selectedIndices.push(idx);
        }
        if (selectedIndices.length === 0) {
          setSelectedIds(new Set([id]));
          return;
        }
        const minSel = Math.min(...selectedIndices);
        const maxSel = Math.max(...selectedIndices);
        const from = Math.min(minSel, clickIndex);
        const to = Math.max(maxSel, clickIndex);
        const next = new Set<TId>();
        for (let i = from; i <= to; i++) {
          const rowId = orderedIds[i];
          if (rowId !== undefined) next.add(rowId);
        }
        setSelectedIds(next);
        return;
      }

      const anchor = selectionAnchorRef.current;
      const anchorIndex =
        anchor != null ? indexById.get(anchor) : undefined;

      if (anchorIndex === undefined) {
        setSelectedIds(new Set([id]));
        selectionAnchorRef.current = id;
        return;
      }

      const from = Math.min(anchorIndex, clickIndex);
      const to = Math.max(anchorIndex, clickIndex);
      const next = new Set<TId>();
      for (let i = from; i <= to; i++) {
        const rowId = orderedIds[i];
        if (rowId !== undefined) next.add(rowId);
      }
      setSelectedIds(next);
      if (rangeUpdatesAnchor) selectionAnchorRef.current = id;
    },
    [indexById, orderedIds, rangeMode, rangeUpdatesAnchor, selectedIds],
  );

  const toggleId = useCallback((id: TId) => {
    setSelectedIds((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
    selectionAnchorRef.current = id;
  }, []);

  const ctrlToggleSelect = useCallback((id: TId) => {
    setSelectedIds((prev) => {
      const next = new Set(prev);
      const focused = focusedIdRef.current;
      if (next.size === 0 && focused != null) {
        if (
          !ctrlSeedSkipsTargetRef.current ||
          !Object.is(focused, id)
        ) {
          next.add(focused);
        }
      }
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
    selectionAnchorRef.current = id;
  }, []);

  const toggleOrRangeSelect = useCallback(
    (id: TId, shiftKey: boolean) => {
      if (shiftKey) {
        applyRangeSelect(id);
        return;
      }
      toggleId(id);
    },
    [applyRangeSelect, toggleId],
  );

  const onSelectColumnClick = useCallback(
    (id: TId, e: MouseEvent) => {
      if (checkboxEvents === "preventAndStop") {
        e.preventDefault();
        e.stopPropagation();
      } else {
        e.stopPropagation();
      }
      if (e.shiftKey) {
        applyRangeSelect(id);
        notifyMutation();
        return;
      }
      if (e.metaKey || e.ctrlKey) {
        ctrlToggleSelect(id);
        notifyMutation();
        return;
      }
      // Match row clicks: with nothing selected, open/focus instead of
      // checking the box (avatar column is easy to hit by accident).
      if (
        selectedIds.size === 0 &&
        rowClickMode !== "alwaysOpen" &&
        onOpenRef.current
      ) {
        onOpenRef.current(id);
        return;
      }
      toggleOrRangeSelect(id, false);
      notifyMutation();
    },
    [
      applyRangeSelect,
      checkboxEvents,
      ctrlToggleSelect,
      notifyMutation,
      rowClickMode,
      selectedIds.size,
      toggleOrRangeSelect,
    ],
  );

  const onRowClick = useCallback(
    (id: TId, e: MouseEvent | { shiftKey: boolean; metaKey?: boolean; ctrlKey?: boolean }) => {
      if (e.shiftKey) {
        if ("preventDefault" in e && typeof e.preventDefault === "function") {
          e.preventDefault();
        }
        applyRangeSelect(id);
        notifyMutation();
        return;
      }
      if (e.metaKey || e.ctrlKey) {
        if ("preventDefault" in e && typeof e.preventDefault === "function") {
          e.preventDefault();
        }
        ctrlToggleSelect(id);
        notifyMutation();
        return;
      }

      if (rowClickMode === "alwaysOpen") {
        onOpenRef.current?.(id);
        return;
      }

      if (rowClickMode === "openWhenEmptyElseToggle") {
        if (selectedIds.size === 0) {
          onOpenRef.current?.(id);
          return;
        }
        toggleOrRangeSelect(id, false);
        notifyMutation();
        return;
      }

      // openWhenEmptyElseToggleIfSelected (Groups): any selection → toggle
      if (selectedIds.size >= 1) {
        toggleOrRangeSelect(id, false);
        notifyMutation();
        return;
      }
      onOpenRef.current?.(id);
    },
    [
      applyRangeSelect,
      ctrlToggleSelect,
      notifyMutation,
      rowClickMode,
      selectedIds.size,
      toggleOrRangeSelect,
    ],
  );

  const toggleSelectAll = useCallback(() => {
    if (allSelected) {
      if (selectAllSetsAnchor) {
        clearSelection();
      } else {
        // Groups: clear selection set only (leave anchor).
        setSelectedIds(new Set());
      }
      return;
    }
    setSelectedIds(new Set(selectAllIds));
    if (selectAllSetsAnchor) {
      selectionAnchorRef.current = selectAllIds[0] ?? null;
    }
  }, [allSelected, clearSelection, selectAllIds, selectAllSetsAnchor]);

  useEffect(() => {
    if (!escapeToClear || selectedIds.size === 0) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      if (escapeBlockedRef.current?.()) return;
      if (escapePreventDefault) e.preventDefault();
      clearSelection();
      // Escape switches keyboard modality; blur so list buttons don't flash a focus ring.
      const el = document.activeElement;
      if (el instanceof HTMLElement) el.blur();
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [escapeToClear, escapePreventDefault, selectedIds.size, clearSelection]);

  return {
    selectedIds,
    setSelectedIds,
    selectionAnchorRef,
    hasSelection,
    multiSelected,
    allSelected,
    someSelected,
    selectAllRef,
    clearSelection,
    toggleSelectAll,
    onSelectColumnClick,
    onRowClick,
  };
}

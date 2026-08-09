"use client";

import { useCallback, useEffect, useRef, useState, type RefObject } from "react";
import type { PanelImperativeHandle } from "react-resizable-panels";

const NAV_COLLAPSED_KEY = "message-vault:navCollapsed";
/** Collapse nav when the viewport is narrower than this. */
const NAV_AUTO_COLLAPSE_BELOW = 900;
/** Matches `w-14` rail / Panel `collapsedSize`. */
export const NAV_RAIL_PX = 56;
const NAV_ANIM_MS = 280;

function easeInOutCubic(t: number) {
  return t < 0.5 ? 4 * t * t * t : 1 - (-2 * t + 2) ** 3 / 2;
}

function nextFrame() {
  return new Promise<void>((resolve) => {
    requestAnimationFrame(() => requestAnimationFrame(() => resolve()));
  });
}

async function animatePanelSize(
  panel: PanelImperativeHandle,
  toPx: number,
  durationMs: number,
) {
  const fromPx = panel.getSize().inPixels;
  if (Math.abs(fromPx - toPx) < 1) {
    panel.resize(toPx);
    return;
  }
  const start = performance.now();
  await new Promise<void>((resolve) => {
    const step = (now: number) => {
      const t = Math.min(1, (now - start) / durationMs);
      const px = fromPx + (toPx - fromPx) * easeInOutCubic(t);
      panel.resize(px);
      if (t < 1) requestAnimationFrame(step);
      else {
        panel.resize(toPx);
        resolve();
      }
    };
    requestAnimationFrame(step);
  });
}

export type NavAnimDirection = "collapse" | "expand" | null;

export function useNavCollapse(
  navPanelRef: RefObject<PanelImperativeHandle | null>,
  onCollapsedChange?: (collapsed: boolean) => void,
  /** While non-null, Panel `minSize` should be 0 so width can animate through the rail size. */
  onAnimatingChange?: (direction: NavAnimDirection) => void,
) {
  const [userCollapsed, setUserCollapsed] = useState(false);
  const [narrow, setNarrow] = useState(false);
  const [forceExpand, setForceExpand] = useState(false);
  const wasNarrowRef = useRef(false);
  const animatingRef = useRef(false);
  const lastExpandedPxRef = useRef(200);

  const collapsed = narrow ? !forceExpand : userCollapsed;

  useEffect(() => {
    onCollapsedChange?.(collapsed);
  }, [collapsed, onCollapsedChange]);

  // Sync panel collapse without animation (mount, resize breakpoint, storage).
  useEffect(() => {
    const panel = navPanelRef.current;
    if (!panel || animatingRef.current) return;
    if (collapsed) {
      panel.collapse();
      return;
    }
    if (panel.isCollapsed()) {
      panel.resize(lastExpandedPxRef.current);
    } else {
      panel.expand();
    }
  }, [collapsed, navPanelRef]);

  useEffect(() => {
    setUserCollapsed(window.localStorage.getItem(NAV_COLLAPSED_KEY) === "1");

    const syncNarrow = () => {
      const next = window.innerWidth < NAV_AUTO_COLLAPSE_BELOW;
      if (next && !wasNarrowRef.current) setForceExpand(false);
      wasNarrowRef.current = next;
      setNarrow(next);
    };
    syncNarrow();
    window.addEventListener("resize", syncNarrow);
    return () => window.removeEventListener("resize", syncNarrow);
  }, []);

  const setCollapsed = useCallback(
    (next: boolean) => {
      if (narrow) {
        setForceExpand(!next);
        return;
      }
      setUserCollapsed(next);
      window.localStorage.setItem(NAV_COLLAPSED_KEY, next ? "1" : "0");
    },
    [narrow],
  );

  const collapse = useCallback(async () => {
    if (narrow) {
      setForceExpand(false);
      return;
    }
    const panel = navPanelRef.current;
    if (!panel || collapsed || animatingRef.current) {
      setCollapsed(true);
      return;
    }

    lastExpandedPxRef.current = Math.max(
      panel.getSize().inPixels,
      NAV_RAIL_PX,
    );
    animatingRef.current = true;
    onAnimatingChange?.("collapse");
    await nextFrame();

    try {
      await animatePanelSize(panel, NAV_RAIL_PX, NAV_ANIM_MS);
      setUserCollapsed(true);
      window.localStorage.setItem(NAV_COLLAPSED_KEY, "1");
    } finally {
      onAnimatingChange?.(null);
      animatingRef.current = false;
    }
  }, [collapsed, narrow, navPanelRef, onAnimatingChange, setCollapsed]);

  const expand = useCallback(async () => {
    if (narrow) {
      setForceExpand(true);
      return;
    }
    const panel = navPanelRef.current;
    if (!panel || !collapsed || animatingRef.current) {
      setCollapsed(false);
      return;
    }

    const targetPx = Math.max(lastExpandedPxRef.current, 200);
    animatingRef.current = true;
    onAnimatingChange?.("expand");
    setUserCollapsed(false);
    window.localStorage.setItem(NAV_COLLAPSED_KEY, "0");
    await nextFrame();

    try {
      if (panel.getSize().inPixels > NAV_RAIL_PX + 1) {
        panel.resize(NAV_RAIL_PX);
        await nextFrame();
      }
      await animatePanelSize(panel, targetPx, NAV_ANIM_MS);
    } finally {
      onAnimatingChange?.(null);
      animatingRef.current = false;
    }
  }, [collapsed, narrow, navPanelRef, onAnimatingChange, setCollapsed]);

  const toggleCollapsed = useCallback(() => {
    if (collapsed) void expand();
    else void collapse();
  }, [collapsed, collapse, expand]);

  return { collapsed, toggleCollapsed, expand, collapse };
}

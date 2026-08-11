/**
 * Selectors for React Aria menus/calendars portaled outside the panel root.
 *
 * Only match marked overlays and modal underlays — not every listbox/option on
 * the page. Contact/conversation lists also use role=listbox/option; treating
 * those as overlays prevented outside-click dismiss of the search popdown.
 */
const PORTALED_OVERLAY_SELECTOR = [
  "[data-mv-overlay]",
  "[data-testid='underlay']",
].join(", ");

function elementFromEventTarget(target: EventTarget | null): Element | null {
  if (target instanceof Element) return target;
  if (target instanceof Node) return target.parentElement;
  return null;
}

/**
 * True when the event target is inside a portaled overlay (or is a text node
 * inside one). Outside-click handlers should ignore these so Select/Date menus
 * do not dismiss the parent panel.
 */
export function isPortaledOverlayTarget(target: EventTarget | null): boolean {
  return Boolean(elementFromEventTarget(target)?.closest(PORTALED_OVERLAY_SELECTOR));
}

/**
 * True when an outside-click should leave a parent panel open.
 *
 * Use from a capture-phase listener so the Select/Date underlay is still in the
 * event path before React Aria removes it. Clicks on the panel root or on a
 * portaled menu/underlay keep the panel open; RAC closes the menu itself.
 */
export function shouldIgnoreOutsideDismiss(
  event: MouseEvent | PointerEvent,
  root: Node | null,
): boolean {
  if (!root) return true;

  const path = event.composedPath();
  for (const node of path) {
    if (!(node instanceof Element)) continue;
    if (node === root || root.contains(node)) return true;
    if (node.closest(PORTALED_OVERLAY_SELECTOR)) return true;
  }

  return false;
}

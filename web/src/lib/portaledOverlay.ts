/** Selectors for React Aria menus/calendars portaled outside the panel root. */
const PORTALED_OVERLAY_SELECTOR =
  "[data-mv-overlay], [role='listbox'], [role='option'], [role='dialog']";

/**
 * True when the event target is inside a portaled overlay (or is a text node
 * inside one). Outside-click handlers should ignore these so Select/Date menus
 * do not dismiss the parent panel.
 */
export function isPortaledOverlayTarget(target: EventTarget | null): boolean {
  let el: Element | null = null;
  if (target instanceof Element) {
    el = target;
  } else if (target instanceof Node) {
    el = target.parentElement;
  }
  return Boolean(el?.closest(PORTALED_OVERLAY_SELECTOR));
}

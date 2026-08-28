import "@testing-library/jest-dom/vitest";

// jsdom has no Web Animations API. React Aria's SelectionIndicator (the sliding
// underline under our tab strips) calls getAnimations() on mount, so without
// this stub any test that renders Tabs throws instead of rendering.
if (typeof Element !== "undefined" && !Element.prototype.getAnimations) {
  Element.prototype.getAnimations = () => [];
}

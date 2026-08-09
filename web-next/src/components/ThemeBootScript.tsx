"use client";

import { useSyncExternalStore } from "react";

const emptySubscribe = () => () => {};

/**
 * Emit the FOUC theme boot script only for the server render and the matching
 * hydration pass. React 19 warns if a `<script>` is created during a later
 * client render (those scripts never execute).
 */
export function ThemeBootScript({ script }: { script: string }) {
  const isServerOrHydration = useSyncExternalStore(
    emptySubscribe,
    () => false,
    () => true,
  );
  if (!isServerOrHydration) return null;
  return <script dangerouslySetInnerHTML={{ __html: script }} />;
}

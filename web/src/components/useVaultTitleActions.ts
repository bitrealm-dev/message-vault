"use client";

import { useCallback, useEffect, useState } from "react";
import { useConfirmDialog } from "./useConfirmDialog";

/** Shared logout / demo-reset actions for Message Vault title menus. */
export function useVaultTitleActions() {
  const { confirm, dialog: confirmDialog } = useConfirmDialog();
  const [demoResetAvailable, setDemoResetAvailable] = useState(false);
  const [resettingDemo, setResettingDemo] = useState(false);
  const [resetError, setResetError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    fetch("/api/demo/reset")
      .then((r) => r.json())
      .then((data: { available?: boolean }) => {
        if (!cancelled) setDemoResetAvailable(data.available === true);
      })
      .catch(() => {
        if (!cancelled) setDemoResetAvailable(false);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const logout = useCallback(async () => {
    try {
      const modeRes = await fetch("/api/auth/mode");
      const modeJson = (await modeRes.json()) as {
        authMode?: string;
        hankoApiUrl?: string;
      };
      if (modeJson.authMode === "hanko" && modeJson.hankoApiUrl) {
        const { Hanko } = await import("@teamhanko/hanko-elements");
        const hanko = new Hanko(modeJson.hankoApiUrl);
        await hanko.logout();
      }
    } catch {
      // Best-effort Hanko logout; still clear the vault cookie.
    }
    await fetch("/api/auth/logout", { method: "POST" });
    window.location.assign("/login");
  }, []);

  const resetDemo = useCallback(async () => {
    setResetError(null);
    await confirm(
      "Demo reset is CLI-only (the web app no longer imports data).\n\nFrom the repo root:\n\ncargo run --release -- reset-demo\n\nThen reload this page (or sign out and back in).",
      "Reset demo",
    );
    setResettingDemo(false);
  }, [confirm]);

  return {
    demoResetAvailable,
    resettingDemo,
    resetError,
    logout,
    resetDemo,
    confirmDialog,
  };
}

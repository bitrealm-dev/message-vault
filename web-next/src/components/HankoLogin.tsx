"use client";

import { Hanko, register } from "@teamhanko/hanko-elements";
import { useEffect, useState } from "react";

type Props = {
  apiUrl: string;
};

export function HankoLogin({ apiUrl }: Props) {
  const [error, setError] = useState<string | null>(null);
  const [bridging, setBridging] = useState(false);

  useEffect(() => {
    if (!apiUrl) return;
    void register(apiUrl).catch((err: unknown) => {
      setError(
        err instanceof Error ? err.message : "Couldn’t load Hanko sign-in.",
      );
    });
  }, [apiUrl]);

  useEffect(() => {
    if (!apiUrl) return;
    const hanko = new Hanko(apiUrl);
    const remove = hanko.onSessionCreated(() => {
      void (async () => {
        setBridging(true);
        setError(null);
        try {
          const res = await fetch("/api/auth/hanko/session", { method: "POST" });
          const json = (await res.json()) as {
            error?: string;
            needsOnboarding?: boolean;
          };
          if (!res.ok) {
            throw new Error(json.error ?? "Couldn’t start vault session");
          }
          window.location.assign(
            json.needsOnboarding ? "/onboarding" : "/",
          );
        } catch (err) {
          setError(
            err instanceof Error ? err.message : "Couldn’t start vault session",
          );
          setBridging(false);
        }
      })();
    });
    return () => {
      remove();
    };
  }, [apiUrl]);

  return (
    <div className="space-y-4">
      <hanko-auth />
      {bridging ? (
        <p className="text-center text-[13px] text-muted">Signing you in…</p>
      ) : null}
      {error ? (
        <p className="text-center text-[13px] text-danger">{error}</p>
      ) : null}
    </div>
  );
}

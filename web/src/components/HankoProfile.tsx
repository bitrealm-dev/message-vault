"use client";

import { register } from "@teamhanko/hanko-elements";
import { useEffect, useState } from "react";

type Props = {
  apiUrl: string;
};

/** Hanko profile widget: manage emails and passkeys while signed in. */
export function HankoProfile({ apiUrl }: Props) {
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!apiUrl) return;
    void register(apiUrl).catch((err: unknown) => {
      setError(
        err instanceof Error ? err.message : "Couldn’t load Hanko profile.",
      );
    });
  }, [apiUrl]);

  if (!apiUrl) return null;

  return (
    <div className="space-y-2">
      <hanko-profile />
      {error ? (
        <p className="text-[13px] text-danger" role="alert">
          {error}
        </p>
      ) : null}
    </div>
  );
}

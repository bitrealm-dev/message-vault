import { useCallback, useEffect, useRef, useState } from "react";
import { apiClient, setBaseUrl } from "../lib/api";
import { useAuth } from "../lib/auth";
import { initialLoginServerUrl, vaultDisplayHost } from "../lib/authGuards";
import { isTauri } from "../lib/tauri-check";
import { authCard, authCardBody, pageCenter } from "../lib/uiStyles";
import { useVaultHealth } from "../lib/useVaultHealth";
import { probeTimeoutSignal, type VaultHealthStatus } from "../lib/vaultHealth";
import LocalAuthTabs from "./auth/LocalAuthTabs";
import VaultLine, { type VaultConnection } from "./auth/VaultLine";

interface AuthModeResponse {
  mode: string;
  try_demo?: boolean;
}

/** Placeholder shaped like the form, so the card does not flicker into shape. */
function FormSkeleton({ dimmed }: { dimmed: boolean }) {
  return (
    <div className={dimmed ? "opacity-40" : ""} aria-hidden="true">
      <div className="mb-6 h-9 rounded bg-elevated" />
      <div className="h-3.5 w-1/3 rounded bg-elevated" />
      <div className="mt-2 h-10 rounded bg-elevated" />
      <div className="mt-5 h-3.5 w-1/4 rounded bg-elevated" />
      <div className="mt-2 h-10 rounded bg-elevated" />
    </div>
  );
}

/**
 * The way into a vault. The card resolves an address on mount and detects the
 * auth mode itself, so the only question the old first screen asked — which
 * vault — is answered by default and changed in place when the default is
 * wrong.
 */
export default function LoginScreen() {
  const { setServer: setAuthServer, serverUrl: savedUrl } = useAuth();
  const [address, setAddress] = useState(() => initialLoginServerUrl(savedUrl, isTauri()));
  const [draft, setDraft] = useState(address);
  const [state, setState] = useState<VaultConnection>("connecting");
  const [authMode, setAuthMode] = useState<"local" | null>(null);

  const editorOpen = state === "editing" || state === "disconnected";
  // Only probe while the address is being chosen. Once connected, the mode
  // request has already proved the vault is there.
  const health = useVaultHealth(editorOpen ? draft : null);

  const connect = useCallback(
    async (url: string) => {
      const trimmed = url.trim();
      setState("connecting");
      setBaseUrl(trimmed);
      try {
        // Hanko has been removed from this product; whatever the vault
        // reports, the only mode this card knows how to render is local.
        await apiClient.get<AuthModeResponse>("/v1/auth/mode", {
          signal: probeTimeoutSignal(),
        });
        setAddress(trimmed);
        setDraft(trimmed);
        setAuthMode("local");
        setAuthServer(trimmed);
        setState("connected");
      } catch {
        // Nothing answered. That is the vault line's problem, not the form's.
        setState("disconnected");
      }
    },
    [setAuthServer],
  );

  // Resolve the vault once on mount; Use and Retry call `connect` again.
  const started = useRef(false);
  useEffect(() => {
    if (started.current) return;
    started.current = true;
    void connect(address);
  }, [connect, address]);

  // A disconnected card heals itself: when the live health probe finds the
  // vault reachable again, reconnect without waiting for Retry. Fires only on
  // the transition into "ok" — not on every render while it stays "ok" — so a
  // `connect()` that fails and lands back in "disconnected" does not
  // immediately retry.
  const previousHealth = useRef<VaultHealthStatus>(health);
  useEffect(() => {
    const becameHealthy = previousHealth.current !== "ok" && health === "ok";
    previousHealth.current = health;
    if (state === "disconnected" && becameHealthy) {
      void connect(draft);
    }
  }, [health, state, draft, connect]);

  const host = vaultDisplayHost(
    state === "connected" ? address : draft,
    typeof window === "undefined" ? "" : window.location.host,
  );

  return (
    <div className={pageCenter}>
      <div className={authCard}>
        <div className={authCardBody}>
          <VaultLine
            state={state}
            host={host}
            draft={draft}
            health={health}
            onDraftChange={setDraft}
            onEdit={() => setState("editing")}
            onCancel={() => {
              setDraft(address);
              setState("connected");
            }}
            onSubmit={() => void connect(draft)}
          />

          {authMode === null ? (
            <FormSkeleton dimmed={state === "disconnected"} />
          ) : (
            <LocalAuthTabs serverUrl={address} disabled={state !== "connected"} />
          )}
        </div>
      </div>
    </div>
  );
}

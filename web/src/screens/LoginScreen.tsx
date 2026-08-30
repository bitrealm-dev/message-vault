import { useCallback, useEffect, useRef, useState } from "react";
import { setBaseUrl } from "../lib/api";
import { useAuth } from "../lib/auth";
import { initialLoginServerUrl } from "../lib/authGuards";
import { isTauri } from "../lib/tauri-check";
import { accentLink, authCard, authCardBody, authScreenTitle, pageCenter } from "../lib/uiStyles";
import { useVaultHealth } from "../lib/useVaultHealth";
import { checkVaultHealth, type VaultHealthStatus } from "../lib/vaultHealth";
import LocalAuthTabs from "./auth/LocalAuthTabs";
import VaultSettingsScreen from "./auth/VaultSettingsScreen";
import VaultStatus, { type VaultConnection } from "./auth/VaultStatus";

/** Placeholder shaped like the form, so the card does not flicker into shape. */
function FormSkeleton({ dimmed }: { dimmed: boolean }) {
  return (
    <div className={`min-h-0 flex-1 ${dimmed ? "opacity-40" : ""}`} aria-hidden="true">
      <div className="mb-6 h-9 rounded bg-elevated" />
      <div className="h-3.5 w-1/3 rounded bg-elevated" />
      <div className="mt-2 h-10 rounded bg-elevated" />
      <div className="mt-5 h-3.5 w-1/4 rounded bg-elevated" />
      <div className="mt-2 h-10 rounded bg-elevated" />
    </div>
  );
}

/** Hairline either side of the word, parting the way in from the way out. */
function OrRule() {
  return (
    <div className="mt-2.5 flex items-center gap-3 text-[0.75rem] text-muted">
      <span className="h-px flex-1 bg-border" />
      or
      <span className="h-px flex-1 bg-border" />
    </div>
  );
}

/**
 * The way into a vault. The card resolves an address on mount and confirms the
 * vault is reachable itself, so the only question the old first screen asked —
 * which vault — is answered by default, reported as a single word under the
 * product name, and changed on a settings screen when the default is wrong.
 */
export default function LoginScreen() {
  const { setServer: setAuthServer, serverUrl: savedUrl } = useAuth();
  const [address, setAddress] = useState(() => initialLoginServerUrl(savedUrl, isTauri()));
  const [draft, setDraft] = useState(address);
  const [state, setState] = useState<VaultConnection>("connecting");
  // Sticky once true: once the sign-in form has been shown, keep showing it
  // (dimmed while disconnected) instead of reverting to the skeleton.
  const [hasConnectedOnce, setHasConnectedOnce] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  // What Test reported for the address currently typed, or null when it has
  // not been tested since the last edit.
  const [tested, setTested] = useState<VaultConnection | null>(null);

  // A disconnected card keeps checking the address it already has, so it can
  // heal itself the moment the vault comes back. Nothing else probes: the
  // settings screen asks explicitly, with Test.
  const health = useVaultHealth(state === "disconnected" ? address : null);

  const connect = useCallback(
    async (url: string) => {
      const trimmed = url.trim();
      setState("connecting");
      setBaseUrl(trimmed);
      // GET /health answers plain text, not JSON, so this probes it directly
      // rather than through apiClient (which always parses the body as
      // JSON). The body is discarded either way — only reachability matters.
      const reachable = await checkVaultHealth(trimmed);
      if (reachable) {
        setAddress(trimmed);
        setDraft(trimmed);
        setHasConnectedOnce(true);
        setAuthServer(trimmed);
        setState("connected");
      } else {
        // Nothing answered. That is the status line's problem, not the form's.
        setState("disconnected");
      }
    },
    [setAuthServer],
  );

  // Resolve the vault once on mount; Change vault address calls `connect` again.
  const started = useRef(false);
  useEffect(() => {
    if (started.current) return;
    started.current = true;
    void connect(address);
  }, [connect, address]);

  // A disconnected card heals itself: when the live health probe finds the
  // vault reachable again, reconnect without waiting to be asked. Fires only on
  // the transition into "ok" — not on every render while it stays "ok" — so a
  // `connect()` that fails and lands back in "disconnected" does not
  // immediately retry.
  const previousHealth = useRef<VaultHealthStatus>(health);
  useEffect(() => {
    const becameHealthy = previousHealth.current !== "ok" && health === "ok";
    previousHealth.current = health;
    if (state === "disconnected" && becameHealthy) {
      void connect(address);
    }
  }, [health, state, address, connect]);

  // Only the newest Test may write the result: an earlier slow probe must not
  // stamp its answer over a later one, or over a screen that has since closed.
  const testRun = useRef(0);
  const runTest = useCallback(async () => {
    const run = testRun.current + 1;
    testRun.current = run;
    setTested("connecting");
    const reachable = await checkVaultHealth(draft.trim());
    if (testRun.current !== run) return;
    setTested(reachable ? "connected" : "disconnected");
  }, [draft]);

  /**
   * What the settings screen reports under Connection Status.
   *
   * Test's answer wins while it lasts. Without one, the card's own connection
   * may be shown only while the box still holds the address that connection
   * was made to — edit a character and `state` is describing a different
   * vault, so repeating it here would tell the person that the address they
   * are typing works, on the strength of a probe that never touched it. That
   * is how a failed Test used to turn green again on the next keystroke.
   */
  const settingsStatus: VaultConnection = tested ?? (draft.trim() === address ? state : "untested");

  const closeSettings = () => {
    testRun.current += 1;
    setTested(null);
    setSettingsOpen(false);
  };

  return (
    <div className={pageCenter}>
      <div className={authCard}>
        <div className={authCardBody}>
          {settingsOpen ? (
            <VaultSettingsScreen
              draft={draft}
              status={settingsStatus}
              onDraftChange={(value) => {
                setDraft(value);
                setTested(null);
              }}
              onTest={() => void runTest()}
              onCancel={() => {
                setDraft(address);
                closeSettings();
              }}
              onSubmit={() => {
                const next = draft;
                closeSettings();
                void connect(next);
              }}
            />
          ) : (
            <>
              <h1 className={`${authScreenTitle} mb-2`}>Message Vault</h1>
              <VaultStatus state={state} className="mb-5 text-center" />

              {hasConnectedOnce ? (
                <LocalAuthTabs serverUrl={address} disabled={state !== "connected"} />
              ) : (
                <FormSkeleton dimmed={state === "disconnected"} />
              )}

              <OrRule />
              <div className="mt-4 text-center">
                <button
                  type="button"
                  className={accentLink}
                  onClick={() => {
                    setDraft(address);
                    setTested(null);
                    setSettingsOpen(true);
                  }}
                >
                  Change vault settings
                </button>
              </div>
            </>
          )}
        </div>
      </div>
    </div>
  );
}

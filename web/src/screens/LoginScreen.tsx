import { useEffect, useRef, useState } from "react";
import AuthBackButton from "../components/AuthBackButton";
import AuthErrorFooter from "../components/AuthErrorFooter";
import Button from "../components/Button";
import HealthDot from "../components/HealthDot";
import TextField from "../components/TextField";
import { apiClient, setBaseUrl } from "../lib/api";
import { useAuth } from "../lib/auth";
import {
  type AuthMode,
  initialLoginServerUrl,
  isAuthMode,
  type SessionResponse,
} from "../lib/authGuards";
import { isTauri } from "../lib/tauri-check";
import { authCard, authTitle, pageCenter } from "../lib/uiStyles";
import { useAsyncAction } from "../lib/useAsyncAction";
import { useVaultHealth } from "../lib/useVaultHealth";
import LocalAuthTabs from "./auth/LocalAuthTabs";

interface AuthModeResponse {
  mode: string;
  hanko_api_url?: string | null;
  try_demo?: boolean;
}

export default function LoginScreen() {
  const { login, setServer: setAuthServer, serverUrl: savedUrl } = useAuth();
  const [serverUrl, setServerUrl] = useState(() => initialLoginServerUrl(savedUrl, isTauri()));
  const [authMode, setAuthMode] = useState<AuthMode | null>(null);
  const [hankoApiUrl, setHankoApiUrl] = useState<string | null>(null);
  const { busy, error, run, clearError } = useAsyncAction();
  const [hankoError, setHankoError] = useState("");

  const hankoRef = useRef<HTMLDivElement>(null);
  // Only probe while choosing a vault; stop after Connect advances the card.
  const healthStatus = useVaultHealth(authMode === null ? serverUrl : null);

  const displayError = error || hankoError;

  const detectMode = () => {
    setAuthMode(null);
    void run(async () => {
      try {
        const url = serverUrl.trim();
        setBaseUrl(url);
        const res = await apiClient.get<AuthModeResponse>("/v1/auth/mode");
        setAuthMode(isAuthMode(res.mode) ? res.mode : null);
        setHankoApiUrl(res.hanko_api_url || null);
        setAuthServer(url);
      } catch {
        throw new Error(
          isTauri()
            ? "Could not reach server. Check the URL and try again."
            : "Could not reach server. Leave the URL blank for this origin (Vite proxy / vault UI), or enter an absolute vault URL.",
        );
      }
    });
  };

  const changeServer = () => {
    setAuthMode(null);
    setHankoApiUrl(null);
    clearError();
    setHankoError("");
  };

  // Load Hanko sign-in when that login mode is selected.
  useEffect(() => {
    if (authMode !== "hanko" || !hankoApiUrl || !hankoRef.current) return;

    let cancelled = false;
    // `loadHanko` is async, so anything it returns is a promise the effect
    // cannot use as a cleanup — the unsubscribe has to be handed back this way
    // or every run leaks a Hanko instance and its session listener.
    let unsubscribe: (() => void) | null = null;

    const loadHanko = async () => {
      try {
        const mod = await import("@teamhanko/hanko-elements");
        if (cancelled) return;

        // Register the Hanko sign-in web component.
        mod.register(hankoApiUrl).catch(() => {
          if (!cancelled) {
            setHankoError("Failed to load Hanko sign-in.");
          }
        });

        // Listen for a successful Hanko session so the app can log in.
        const hanko = new mod.Hanko(hankoApiUrl);

        const remove = hanko.onSessionCreated(() => {
          if (cancelled) return;
          void run(async () => {
            const jwt = hanko.getSessionToken();
            setBaseUrl(serverUrl.trim());
            const res = await apiClient.post<SessionResponse>("/v1/auth/hanko/session", {
              hanko_jwt: jwt,
            });
            await login(serverUrl.trim(), res.token, res.account_id);
          });
        });

        if (cancelled) {
          remove();
          return;
        }
        unsubscribe = remove;
      } catch {
        if (!cancelled) {
          setHankoError("Failed to load Hanko. Is @teamhanko/hanko-elements installed?");
        }
      }
    };

    void loadHanko();

    return () => {
      cancelled = true;
      unsubscribe?.();
    };
  }, [authMode, hankoApiUrl, serverUrl, login, run]);

  return (
    <div className={pageCenter}>
      <div className={authCard}>
        {/* In local mode the tab strip is the card's heading. */}
        {authMode !== "local" && (
          <h1 className={authMode === null ? `${authTitle} !text-center` : authTitle}>
            {authMode === null ? "Message Vault" : "Sign In"}
          </h1>
        )}

        {authMode === null && (
          <>
            <TextField
              label="Server URL"
              labelEnd={<HealthDot status={healthStatus} />}
              value={serverUrl}
              onChange={setServerUrl}
              onKeyDown={(e) => e.key === "Enter" && detectMode()}
              placeholder={isTauri() ? "https://vault.example.com" : "Leave blank for this origin"}
            />
            <div className="mt-3 mb-[0.35rem] flex justify-end">
              <Button variant="primary" onClick={detectMode} disabled={busy}>
                {busy ? "Connecting…" : "Connect"}
              </Button>
            </div>
            {!isTauri() && (
              <p className="text-[0.75rem] text-muted mb-4">
                Leave blank to use this origin (Vite `/v1` proxy or vault-hosted UI).
              </p>
            )}

            <AuthErrorFooter error={displayError} />
          </>
        )}

        {authMode === "local" && <LocalAuthTabs serverUrl={serverUrl} />}

        {authMode === "hanko" && (
          <>
            <div ref={hankoRef}>
              {hankoApiUrl ? (
                <hanko-auth />
              ) : (
                <div className="text-[0.875rem] text-muted text-center p-4">
                  Hanko API URL not configured on server.
                </div>
              )}
            </div>

            <AuthErrorFooter error={displayError} />
          </>
        )}

        {authMode !== null && (
          <AuthBackButton label="Back to Vault Selection" onClick={changeServer} />
        )}
      </div>
    </div>
  );
}

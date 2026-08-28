import { useEffect, useRef, useState } from "react";
import { useNavigate } from "react-router-dom";
import AuthBackButton from "../components/AuthBackButton";
import AuthErrorFooter from "../components/AuthErrorFooter";
import Button from "../components/Button";
import HealthDot from "../components/HealthDot";
import PasswordField from "../components/PasswordField";
import TextField from "../components/TextField";
import { apiClient, setBaseUrl } from "../lib/api";
import { useAuth } from "../lib/auth";
import { type AuthMode, initialLoginServerUrl, isAuthMode } from "../lib/authGuards";
import { isTauri } from "../lib/tauri-check";
import { authCard, authTitle, mutedText, pageCenter } from "../lib/uiStyles";
import { useAsyncAction } from "../lib/useAsyncAction";
import { useVaultHealth } from "../lib/useVaultHealth";

interface AuthModeResponse {
  mode: string;
  hanko_api_url?: string | null;
  try_demo?: boolean;
}

/** Flip to true to allow demo sign-in from the login cards. */
const TRY_IT_ENABLED = false;

export default function LoginScreen() {
  const navigate = useNavigate();
  const { login, setServer: setAuthServer, serverUrl: savedUrl } = useAuth();
  const [serverUrl, setServerUrl] = useState(() => initialLoginServerUrl(savedUrl, isTauri()));
  const [authMode, setAuthMode] = useState<AuthMode | null>(null);
  const [hankoApiUrl, setHankoApiUrl] = useState<string | null>(null);
  const { busy, error, run, clearError } = useAsyncAction();
  const [hankoError, setHankoError] = useState("");

  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [showPassword, setShowPassword] = useState(false);

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

  const handleTryDemo = () => {
    void run(async () => {
      const url = serverUrl.trim();
      setBaseUrl(url);
      const res = await apiClient.post<{
        token: string;
        account_id: string;
      }>("/v1/auth/try-demo", {});
      login(url, res.token, res.account_id);
    });
  };

  const handleLocalLogin = () => {
    void run(async () => {
      if (!username.trim()) {
        throw new Error("Username is required.");
      }
      const res = await apiClient.post<{
        token: string;
        account_id: string;
      }>("/v1/auth/login", { username, password });
      login(serverUrl.trim(), res.token, res.account_id);
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
            const res = await apiClient.post<{
              token: string;
              account_id: string;
            }>("/v1/auth/hanko/session", { hanko_jwt: jwt });
            login(serverUrl.trim(), res.token, res.account_id);
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
        <h1 className={authMode === null ? `${authTitle} !text-center` : authTitle}>
          {authMode === null ? "Message Vault" : "Sign In"}
        </h1>

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
            <TryItFooter busy={busy} onClick={handleTryDemo} />
          </>
        )}

        {authMode === "local" && (
          <>
            <TextField
              label="Username"
              value={username}
              onChange={setUsername}
              onKeyDown={(e) => e.key === "Enter" && handleLocalLogin()}
              autoComplete="username"
            />

            <PasswordField
              label="Password"
              className="mt-3"
              value={password}
              onChange={setPassword}
              onKeyDown={(e) => e.key === "Enter" && handleLocalLogin()}
              autoComplete="current-password"
              showPassword={showPassword}
              onToggle={() => setShowPassword((v) => !v)}
            />

            <div className="mt-6 flex justify-end gap-3">
              <Button onClick={() => navigate("/register")}>Create an account</Button>
              <Button variant="primary" onClick={handleLocalLogin} disabled={busy}>
                {busy ? "Signing in…" : "Sign in"}
              </Button>
            </div>

            <AuthErrorFooter error={displayError} />
            <TryItFooter busy={busy} onClick={handleTryDemo} />
          </>
        )}

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
            <TryItFooter busy={busy} onClick={handleTryDemo} />
          </>
        )}

        {authMode !== null && (
          <AuthBackButton label="Back to Vault Selection" onClick={changeServer} />
        )}
      </div>
    </div>
  );
}

function TryItFooter({ busy, onClick }: { busy: boolean; onClick: () => void }) {
  const caption = TRY_IT_ENABLED
    ? "Open a sample account."
    : "Sample sign-in is temporarily unavailable.";
  return (
    <>
      <div className={`${orRowClass} mb-2 mt-3`}>
        <span className={orLineClass} />
        <span className={orTextClass}>OR</span>
        <span className={orLineClass} />
      </div>
      <TryItButton busy={busy} onClick={onClick} />
      <p className={`${mutedText} mt-2`}>{caption}</p>
    </>
  );
}

function TryItButton({ busy, onClick }: { busy: boolean; onClick: () => void }) {
  return (
    <Button
      variant="primary"
      onClick={onClick}
      disabled={!TRY_IT_ENABLED || busy}
      title={TRY_IT_ENABLED ? undefined : "Sample sign-in is temporarily unavailable."}
    >
      {busy ? "Opening sample…" : "Try it"}
    </Button>
  );
}

const orRowClass = "flex items-center gap-3";

const orLineClass = "h-px flex-1 bg-border";

const orTextClass = "text-[0.75rem] font-medium text-muted";

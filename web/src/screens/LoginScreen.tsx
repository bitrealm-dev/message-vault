import { useEffect, useRef, useState } from "react";
import { useNavigate } from "react-router-dom";
import AuthBackButton from "../components/AuthBackButton";
import AuthErrorFooter from "../components/AuthErrorFooter";
import Button from "../components/Button";
import PasswordField from "../components/PasswordField";
import TextField from "../components/TextField";
import { apiClient, setBaseUrl } from "../lib/api";
import { useAuth } from "../lib/auth";
import { type AuthMode, initialLoginServerUrl, isAuthMode } from "../lib/authGuards";
import { isTauri } from "../lib/tauri-check";
import { authCard, authLabel, authTitle, mutedText, pageCenter } from "../lib/uiStyles";
import { useAsyncAction } from "../lib/useAsyncAction";
import ExtractScreen from "./Extract";
import FormatScreen from "./Format";

interface AuthModeResponse {
  mode: string;
  hanko_api_url?: string | null;
  try_demo?: boolean;
}

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
  const [offlineScreen, setOfflineScreen] = useState<"none" | "extract" | "format">("none");

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

        return () => {
          remove();
        };
      } catch {
        if (!cancelled) {
          setHankoError("Failed to load Hanko. Is @teamhanko/hanko-elements installed?");
        }
      }
    };

    loadHanko();

    return () => {
      cancelled = true;
    };
  }, [authMode, hankoApiUrl, serverUrl, login, run]);

  if (offlineScreen === "extract") {
    return <ExtractScreen onBack={() => setOfflineScreen("none")} />;
  }
  if (offlineScreen === "format") {
    return <FormatScreen onBack={() => setOfflineScreen("none")} />;
  }

  return (
    <div className={pageCenter}>
      <div className={authCard}>
        <h1 className={authTitle}>{authMode === null ? "Message Vault" : "Sign In"}</h1>

        {authMode === null && (
          <>
            <div className="mb-4">
              <TryItButton busy={busy} onClick={handleTryDemo} />
              <p className={`${mutedText} mt-2`}>Open a sample account.</p>
            </div>
            <div className={`${orRowClass} mb-4`}>
              <span className={orLineClass} />
              <span className={orTextClass}>OR</span>
              <span className={orLineClass} />
            </div>
            <label className={authLabel}>Server URL</label>
            <TextField
              value={serverUrl}
              onChange={setServerUrl}
              onKeyDown={(e) => e.key === "Enter" && detectMode()}
              placeholder={isTauri() ? "https://vault.example.com" : "Leave blank for this origin"}
            />
            <div className="mt-3 mb-[0.35rem] flex justify-end">
              <Button
                variant="primary"
                onClick={detectMode}
                disabled={busy}
                className="!px-4 !py-2"
              >
                {busy ? "Connecting…" : "Connect"}
              </Button>
            </div>
            {!isTauri() && (
              <p className="text-[0.75rem] text-muted mb-4">
                Leave blank to use this origin (Vite `/v1` proxy or vault-hosted UI).
              </p>
            )}
            {isTauri() && <div className="mb-4" />}

            {isTauri() && (
              <>
                <div className={`${orRowClass} mb-2 mt-3`}>
                  <span className={orLineClass} />
                  <span className={orTextClass}>OR</span>
                  <span className={orLineClass} />
                </div>
                <p className={`${mutedText} text-center mb-2`}>Use offline message tools.</p>
                <div className="flex gap-3">
                  <Button onClick={() => setOfflineScreen("extract")} className="flex-1 !p-2">
                    Extract messages
                  </Button>
                  <Button onClick={() => setOfflineScreen("format")} className="flex-1 !p-2">
                    Format conversion
                  </Button>
                </div>
              </>
            )}

            <AuthErrorFooter error={displayError} />
          </>
        )}

        {authMode === "local" && (
          <>
            <div className="mb-4">
              <TryItButton busy={busy} onClick={handleTryDemo} />
              <p className={`${mutedText} mt-2`}>Open a sample account.</p>
            </div>
            <label className={authLabel}>Username</label>
            <TextField
              value={username}
              onChange={setUsername}
              onKeyDown={(e) => e.key === "Enter" && handleLocalLogin()}
              autoComplete="username"
            />

            <label className={`${authLabel} mt-3`}>Password</label>
            <PasswordField
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
          </>
        )}

        {authMode === "hanko" && (
          <>
            <div className="mb-4">
              <TryItButton busy={busy} onClick={handleTryDemo} />
              <p className={`${mutedText} mt-2`}>Open a sample account.</p>
            </div>
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

function TryItButton({ busy, onClick }: { busy: boolean; onClick: () => void }) {
  return (
    <Button variant="primary" onClick={onClick} disabled={busy}>
      {busy ? "Opening sample…" : "Try it"}
    </Button>
  );
}

const orRowClass = "flex items-center gap-3";

const orLineClass = "h-px flex-1 bg-border";

const orTextClass = "text-[0.75rem] font-medium text-muted";

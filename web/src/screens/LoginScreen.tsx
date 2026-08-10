import { useState, useEffect, useRef } from "react";
import { useNavigate } from "react-router-dom";
import { useAuth } from "../lib/auth";
import { apiClient, setBaseUrl } from "../lib/api";
import { isTauri } from "../lib/tauri-check";
import PasswordField from "../components/PasswordField";
import AuthSubmitButton from "../components/AuthSubmitButton";
import AuthBackButton from "../components/AuthBackButton";
import Button from "../components/Button";
import {
  accentLink,
  authCard,
  authInput,
  authLabel,
  authTitle,
  mutedText,
  pageCenter,
} from "../lib/uiStyles";
import ExtractScreen from "./Extract";
import FormatScreen from "./Format";

type AuthMode = "hanko" | "local" | null;

interface AuthModeResponse {
  mode: string;
  hanko_api_url?: string | null;
}

export default function LoginScreen() {
  const navigate = useNavigate();
  const { login, setServer: setAuthServer, serverUrl: savedUrl } = useAuth();
  const [serverUrl, setServerUrl] = useState(() => {
    if (typeof savedUrl === "string" && savedUrl.length > 0) return savedUrl;
    return isTauri() ? "http://localhost:8080" : "";
  });
  const [authMode, setAuthMode] = useState<AuthMode>(null);
  const [hankoApiUrl, setHankoApiUrl] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");

  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");

  const hankoRef = useRef<HTMLDivElement>(null);
  const [offlineScreen, setOfflineScreen] = useState<"none" | "extract" | "format">("none");

  const detectMode = async () => {
    setLoading(true);
    setError("");
    setAuthMode(null);
    try {
      const url = serverUrl.trim();
      setBaseUrl(url);
      const res = await apiClient.get<AuthModeResponse>("/v1/auth/mode");
      setAuthMode(res.mode as AuthMode);
      setHankoApiUrl(res.hanko_api_url || null);
      setAuthServer(url);
    } catch {
      setError(
        isTauri()
          ? "Could not reach server. Check the URL and try again."
          : "Could not reach server. Leave the URL blank for this origin (Vite proxy / vault UI), or enter an absolute vault URL.",
      );
    } finally {
      setLoading(false);
    }
  };

  const handleLocalLogin = async () => {
    if (!username.trim()) {
      setError("Username is required.");
      return;
    }
    setLoading(true);
    setError("");
    try {
      const res = await apiClient.post<{
        token: string;
        account_id: string;
      }>("/v1/auth/login", { username, password });
      login(serverUrl.trim(), res.token, res.account_id);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  };

  const changeServer = () => {
    setAuthMode(null);
    setHankoApiUrl(null);
    setError("");
  };

  // Wire Hanko elements when in hanko mode
  useEffect(() => {
    if (authMode !== "hanko" || !hankoApiUrl || !hankoRef.current) return;

    let cancelled = false;

    const loadHanko = async () => {
      try {
        const mod = await import("@teamhanko/hanko-elements");
        if (cancelled) return;

        // Register the Hanko auth web component
        mod.register(hankoApiUrl).catch(() => {
          if (!cancelled) {
            setError("Failed to load Hanko sign-in.");
          }
        });

        // Create Hanko API instance for session handling
        const hanko = new mod.Hanko(hankoApiUrl);

        const remove = hanko.onSessionCreated(() => {
          if (cancelled) return;
          void (async () => {
            try {
              const jwt = hanko.getSessionToken();
              setBaseUrl(serverUrl.trim());
              const res = await apiClient.post<{
                token: string;
                account_id: string;
              }>("/v1/auth/hanko/session", { hanko_jwt: jwt });
              login(serverUrl.trim(), res.token, res.account_id);
            } catch (e) {
              setError(`Hanko login failed: ${e}`);
            }
          })();
        });

        return () => {
          remove();
        };
      } catch {
        if (!cancelled) {
          setError("Failed to load Hanko. Is @teamhanko/hanko-elements installed?");
        }
      }
    };

    loadHanko();

    return () => {
      cancelled = true;
    };
  }, [authMode, hankoApiUrl, serverUrl, login]);

  if (offlineScreen === "extract") {
    return <ExtractScreen onBack={() => setOfflineScreen("none")} />;
  }
  if (offlineScreen === "format") {
    return <FormatScreen onBack={() => setOfflineScreen("none")} />;
  }

  return (
    <div style={pageCenter}>
      <div style={authCard}>
        <h1 style={authTitle}>
          {authMode === null ? "Message Vault" : "Sign In"}
        </h1>

        {authMode === null && (
          <>
            <label style={authLabel}>Server URL</label>
            <div style={{ display: "flex", gap: "0.5rem", marginBottom: "0.35rem" }}>
              <input
                type="text"
                value={serverUrl}
                onChange={(e) => setServerUrl(e.target.value)}
                onKeyDown={(e) => e.key === "Enter" && detectMode()}
                placeholder={
                  isTauri()
                    ? "https://vault.example.com"
                    : "Leave blank for this origin"
                }
                style={{ ...authInput, flex: 1, width: "auto" }}
              />
              <Button
                variant="primary"
                onClick={detectMode}
                disabled={loading}
                style={{ padding: "0.5rem 1rem" }}
              >
                {loading ? "Connecting…" : "Connect"}
              </Button>
            </div>
            {!isTauri() && (
              <p style={{ ...mutedText, fontSize: "0.75rem", marginBottom: "1rem" }}>
                Leave blank to use this origin (Vite `/v1` proxy or vault-hosted UI).
              </p>
            )}
            {isTauri() && <div style={{ marginBottom: "1rem" }} />}

            {isTauri() && (
              <>
                <div style={{ ...orRowStyle, margin: "0.75rem 0 0.5rem" }}>
                  <span style={orLineStyle} />
                  <span style={orTextStyle}>OR</span>
                  <span style={orLineStyle} />
                </div>
                <p
                  style={{
                    ...mutedText,
                    fontSize: "0.813rem",
                    textAlign: "center",
                    margin: "0 0 0.5rem",
                  }}
                >
                  Use offline message tools.
                </p>
                <div style={{ display: "flex", gap: "0.75rem" }}>
                  <Button
                    onClick={() => setOfflineScreen("extract")}
                    style={{ flex: 1, padding: "0.5rem" }}
                  >
                    Extract messages
                  </Button>
                  <Button
                    onClick={() => setOfflineScreen("format")}
                    style={{ flex: 1, padding: "0.5rem" }}
                  >
                    Format conversion
                  </Button>
                </div>
              </>
            )}

            <ErrorFooter error={error} />
          </>
        )}

        {authMode === "local" && (
          <>
            <label style={authLabel}>Username</label>
            <input
              type="text"
              value={username}
              onChange={(e) => setUsername(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && handleLocalLogin()}
              style={authInput}
              autoComplete="username"
            />

            <label style={{ ...authLabel, marginTop: "0.75rem" }}>Password</label>
            <PasswordField
              value={password}
              onChange={setPassword}
              onKeyDown={(e) => e.key === "Enter" && handleLocalLogin()}
            />

            <AuthSubmitButton
              onClick={handleLocalLogin}
              disabled={loading}
            >
              {loading ? "Signing in…" : "Sign in"}
            </AuthSubmitButton>

            <>
              <div style={orRowStyle}>
                <span style={orLineStyle} />
                <span style={orTextStyle}>OR</span>
                <span style={orLineStyle} />
              </div>
              <button type="button" onClick={() => navigate("/register")} style={accentLink}>
                Create an account
              </button>
            </>

            <ErrorFooter error={error} />
          </>
        )}

        {authMode === "hanko" && (
          <>
            <div ref={hankoRef}>
              {hankoApiUrl ? (
                <hanko-auth />
              ) : (
                <div
                  style={{
                    ...mutedText,
                    textAlign: "center",
                    padding: "1rem",
                    fontSize: "0.875rem",
                  }}
                >
                  Hanko API URL not configured on server.
                </div>
              )}
            </div>

            <ErrorFooter error={error} />
          </>
        )}

        {authMode !== null && (
          <AuthBackButton label="Back to Vault Selection" onClick={changeServer} />
        )}
      </div>
    </div>
  );
}

function ErrorFooter({ error }: { error: string }) {
  return (
    <div
      style={{
        marginTop: "1.25rem",
        minHeight: "2.5rem",
        fontSize: "0.813rem",
        lineHeight: 1.35,
        color: error ? "var(--danger)" : "transparent",
      }}
      aria-live="polite"
    >
      {error || "\u00a0"}
    </div>
  );
}

const orRowStyle: React.CSSProperties = {
  display: "flex",
  alignItems: "center",
  gap: "0.75rem",
  margin: "1rem 0 0.75rem",
};

const orLineStyle: React.CSSProperties = {
  flex: 1,
  height: 1,
  background: "var(--border)",
};

const orTextStyle: React.CSSProperties = {
  fontSize: "0.75rem",
  color: "var(--muted)",
  fontWeight: 500,
};

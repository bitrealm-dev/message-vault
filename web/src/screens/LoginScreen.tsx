import { useState, useEffect, useRef } from "react";
import { useAuth } from "../lib/auth";
import { apiClient, setBaseUrl } from "../lib/api";
import { isTauri } from "../lib/tauri-check";
import ExtractScreen from "./Extract";
import FormatScreen from "./Format";

type AuthMode = "hanko" | "local" | null;

interface AuthModeResponse {
  mode: string;
  hanko_api_url?: string | null;
}

export default function LoginScreen({
  onRegister,
}: {
  onRegister?: () => void;
}) {
  const { login, setServer: setAuthServer, serverUrl: savedUrl } = useAuth();
  const [serverUrl, setServerUrl] = useState(savedUrl || "http://localhost:8080");
  const [authMode, setAuthMode] = useState<AuthMode>(null);
  const [hankoApiUrl, setHankoApiUrl] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");

  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");

  const hankoRef = useRef<HTMLDivElement>(null);
  const [offlineScreen, setOfflineScreen] = useState<"none" | "extract" | "format">("none");

  const detectMode = async () => {
    if (!serverUrl.trim()) return;
    setLoading(true);
    setError("");
    setAuthMode(null);
    try {
      setBaseUrl(serverUrl);
      const res = await apiClient.get<AuthModeResponse>("/v1/auth/mode");
      setAuthMode(res.mode as AuthMode);
      setHankoApiUrl(res.hanko_api_url || null);
      setAuthServer(serverUrl);
    } catch {
      setError("Could not reach server. Check the URL and try again.");
    } finally {
      setLoading(false);
    }
  };

  const handleLocalLogin = async () => {
    setLoading(true);
    setError("");
    try {
      const res = await apiClient.post<{
        token: string;
        account_id: string;
      }>("/v1/auth/login", { username, password });
      login(serverUrl, res.token, res.account_id);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
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
              setBaseUrl(serverUrl);
              const res = await apiClient.post<{
                token: string;
                account_id: string;
              }>("/v1/auth/hanko/session", { hanko_jwt: jwt });
              login(serverUrl, res.token, res.account_id);
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
    <div
      style={{
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        minHeight: "100vh",
        background: "#f3f4f6",
        fontFamily: "system-ui",
      }}
    >
      <div
        style={{
          background: "#fff",
          padding: "2rem",
          borderRadius: "8px",
          width: "100%",
          maxWidth: "400px",
          boxShadow: "0 1px 3px rgba(0,0,0,0.1)",
        }}
      >
        <h1
          style={{
            margin: "0 0 1.5rem",
            fontSize: "1.5rem",
            textAlign: "center",
          }}
        >
          Message Vault
        </h1>

        <label
          style={{
            fontSize: "0.875rem",
            fontWeight: 500,
            display: "block",
            marginBottom: "0.25rem",
          }}
        >
          Server URL
        </label>
        <div style={{ display: "flex", gap: "0.5rem", marginBottom: "1rem" }}>
          <input
            type="text"
            value={serverUrl}
            onChange={(e) => setServerUrl(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && detectMode()}
            placeholder="https://vault.example.com"
            style={{
              flex: 1,
              padding: "0.5rem",
              fontSize: "0.875rem",
              border: "1px solid #d1d5db",
              borderRadius: "4px",
            }}
          />
          <button
            onClick={detectMode}
            disabled={loading}
            style={{
              padding: "0.5rem 1rem",
              fontSize: "0.875rem",
              fontWeight: 600,
            }}
          >
            Connect
          </button>
        </div>

        {error && (
          <div
            style={{
              padding: "0.5rem 0.75rem",
              background: "#fef2f2",
              border: "1px solid #fecaca",
              borderRadius: "4px",
              color: "#991b1b",
              fontSize: "0.813rem",
              marginBottom: "1rem",
            }}
          >
            {error}
          </div>
        )}

        {authMode === "local" && (
          <>
            <label
              style={{
                fontSize: "0.875rem",
                fontWeight: 500,
                display: "block",
                marginBottom: "0.25rem",
              }}
            >
              Username
            </label>
            <input
              type="text"
              value={username}
              onChange={(e) => setUsername(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && handleLocalLogin()}
              style={inputStyle}
            />

            <label
              style={{
                fontSize: "0.875rem",
                fontWeight: 500,
                display: "block",
                marginBottom: "0.25rem",
                marginTop: "0.75rem",
              }}
            >
              Password
            </label>
            <input
              type="password"
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && handleLocalLogin()}
              style={{ ...inputStyle, marginBottom: "1rem" }}
            />

            <button
              onClick={handleLocalLogin}
              disabled={loading || !username || !password}
              style={{
                width: "100%",
                padding: "0.75rem",
                fontSize: "1rem",
                fontWeight: 600,
              }}
            >
              {loading ? "Signing in…" : "Sign in"}
            </button>

            {onRegister && (
              <button
                onClick={onRegister}
                style={{
                  width: "100%",
                  padding: "0.5rem",
                  fontSize: "0.875rem",
                  marginTop: "0.5rem",
                  background: "transparent",
                  border: "1px solid #d1d5db",
                  borderRadius: "4px",
                  color: "#4f46e5",
                  cursor: "pointer",
                }}
              >
                Create account
              </button>
            )}
          </>
        )}

        {authMode === "hanko" && (
          <div ref={hankoRef}>
            {hankoApiUrl ? (
              <hanko-auth />
            ) : (
              <div
                style={{
                  textAlign: "center",
                  padding: "1rem",
                  color: "#6b7280",
                  fontSize: "0.875rem",
                }}
              >
                Hanko API URL not configured on server.
              </div>
            )}
          </div>
        )}

        {isTauri() && (
          <>
            <hr
              style={{
                margin: "1.5rem 0",
                border: "none",
                borderTop: "1px solid #e5e7eb",
              }}
            />
            <p
              style={{
                fontSize: "0.813rem",
                color: "#6b7280",
                textAlign: "center",
                marginBottom: "0.75rem",
              }}
            >
              No vault? Use offline tools instead.
            </p>
            <div style={{ display: "flex", gap: "0.75rem" }}>
              <button
                onClick={() => setOfflineScreen("extract")}
                style={{ flex: 1, padding: "0.5rem", fontSize: "0.875rem" }}
              >
                Extract messages
              </button>
              <button
                onClick={() => setOfflineScreen("format")}
                style={{ flex: 1, padding: "0.5rem", fontSize: "0.875rem" }}
              >
                Format conversion
              </button>
            </div>
          </>
        )}
      </div>
    </div>
  );
}

const inputStyle: React.CSSProperties = {
  width: "100%",
  padding: "0.5rem",
  fontSize: "0.875rem",
  border: "1px solid #d1d5db",
  borderRadius: "4px",
  boxSizing: "border-box",
};

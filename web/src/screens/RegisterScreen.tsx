import { useState } from "react";
import { useAuth } from "../lib/auth";
import { apiClient, setBaseUrl } from "../lib/api";

export default function RegisterScreen({
  serverUrl,
  onBack,
}: {
  serverUrl: string;
  onBack: () => void;
}) {
  const { login } = useAuth();
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [confirmPassword, setConfirmPassword] = useState("");
  const [noPassword, setNoPassword] = useState(false);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");

  const handleRegister = async () => {
    setError("");

    if (!username.trim()) {
      setError("Username is required.");
      return;
    }
    if (!noPassword && password !== confirmPassword) {
      setError("Passwords do not match.");
      return;
    }

    setLoading(true);
    try {
      setBaseUrl(serverUrl);
      const res = await apiClient.post<{
        token: string;
        account_id: string;
        username: string;
      }>("/v1/auth/register", {
        username: username.trim(),
        password: noPassword ? "" : password,
      });
      login(serverUrl, res.token, res.account_id, true);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  };

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
          Create Account
        </h1>

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
          placeholder="alphanumeric, _, -, ."
          style={inputStyle}
        />

        <div
          style={{
            marginTop: "1rem",
            display: "flex",
            alignItems: "center",
            gap: "0.5rem",
          }}
        >
          <input
            type="checkbox"
            id="no-password"
            checked={noPassword}
            onChange={(e) => {
              setNoPassword(e.target.checked);
              if (e.target.checked) {
                setPassword("");
                setConfirmPassword("");
              }
            }}
          />
          <label
            htmlFor="no-password"
            style={{ fontSize: "0.875rem", cursor: "pointer" }}
          >
            No password (anyone can sign in with just the username)
          </label>
        </div>

        {!noPassword && (
          <>
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
              Confirm Password
            </label>
            <input
              type="password"
              value={confirmPassword}
              onChange={(e) => setConfirmPassword(e.target.value)}
              style={inputStyle}
            />
          </>
        )}

        {error && (
          <div
            style={{
              padding: "0.5rem 0.75rem",
              background: "#fef2f2",
              border: "1px solid #fecaca",
              borderRadius: "4px",
              color: "#991b1b",
              fontSize: "0.813rem",
              marginTop: "1rem",
            }}
          >
            {error}
          </div>
        )}

        <button
          onClick={handleRegister}
          disabled={loading || !username.trim()}
          style={{
            width: "100%",
            padding: "0.75rem",
            fontSize: "1rem",
            fontWeight: 600,
            marginTop: "1rem",
          }}
        >
          {loading ? "Creating account…" : "Create account"}
        </button>

        <button
          onClick={onBack}
          style={{
            width: "100%",
            padding: "0.5rem",
            fontSize: "0.875rem",
            marginTop: "0.5rem",
            background: "transparent",
            border: "none",
            color: "#4f46e5",
            cursor: "pointer",
          }}
        >
          ← Back to login
        </button>
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

import { useState } from "react";
import { useAuth } from "../lib/auth";
import { apiClient, setBaseUrl } from "../lib/api";
import PasswordField from "../components/PasswordField";

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
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");

  const handleRegister = async () => {
    setError("");

    if (!username.trim()) {
      setError("Username is required.");
      return;
    }
    // Blank passwords are allowed; only reject when the two fields disagree.
    if (password !== confirmPassword) {
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
        password,
        preferred_name: null,
        phone: null,
      });
      login(serverUrl, res.token, res.account_id);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  };

  return (
    <div style={pageStyle}>
      <div style={cardStyle}>
        <h1 style={titleStyle}>Create Account</h1>

        <label style={labelStyle}>Username</label>
        <input
          type="text"
          value={username}
          onChange={(e) => setUsername(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && handleRegister()}
          autoComplete="username"
          style={inputStyle}
        />

        <label style={{ ...labelStyle, marginTop: "0.75rem" }}>Password</label>
        <PasswordField
          value={password}
          onChange={setPassword}
          onKeyDown={(e) => e.key === "Enter" && handleRegister()}
          autoComplete="new-password"
        />

        <label style={{ ...labelStyle, marginTop: "0.75rem" }}>Confirm Password</label>
        <PasswordField
          value={confirmPassword}
          onChange={setConfirmPassword}
          onKeyDown={(e) => e.key === "Enter" && handleRegister()}
          autoComplete="new-password"
        />

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

        <button type="button" onClick={onBack} style={backLinkStyle}>
          ← Back to login
        </button>

        <div
          style={{
            marginTop: "1.25rem",
            minHeight: "2.5rem",
            fontSize: "0.813rem",
            lineHeight: 1.35,
            color: error ? "#991b1b" : "transparent",
          }}
          aria-live="polite"
        >
          {error || "\u00a0"}
        </div>
      </div>
    </div>
  );
}

const pageStyle: React.CSSProperties = {
  display: "flex",
  alignItems: "center",
  justifyContent: "center",
  minHeight: "100vh",
  background: "#f3f4f6",
  fontFamily: "system-ui",
};

const cardStyle: React.CSSProperties = {
  background: "#fff",
  padding: "2rem",
  borderRadius: "8px",
  width: "100%",
  maxWidth: "400px",
  boxShadow: "0 1px 3px rgba(0,0,0,0.1)",
};

const titleStyle: React.CSSProperties = {
  margin: "0 0 1.5rem",
  fontSize: "1.5rem",
  textAlign: "center",
};

const labelStyle: React.CSSProperties = {
  fontSize: "0.875rem",
  fontWeight: 500,
  display: "block",
  marginBottom: "0.25rem",
};

const inputStyle: React.CSSProperties = {
  width: "100%",
  padding: "0.5rem",
  fontSize: "0.875rem",
  border: "1px solid #d1d5db",
  borderRadius: "4px",
  boxSizing: "border-box",
};

const backLinkStyle: React.CSSProperties = {
  display: "block",
  width: "100%",
  marginTop: "0.75rem",
  padding: "0.25rem",
  fontSize: "0.875rem",
  background: "transparent",
  border: "none",
  color: "#4f46e5",
  textDecoration: "underline",
  cursor: "pointer",
  textAlign: "center",
};

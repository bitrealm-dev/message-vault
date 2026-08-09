import { useState } from "react";
import { useAuth } from "../lib/auth";
import { apiClient, setBaseUrl } from "../lib/api";
import PasswordField from "../components/PasswordField";
import AuthSubmitButton from "../components/AuthSubmitButton";
import {
  accentLink,
  authCard,
  authInput,
  authLabel,
  authTitle,
  pageCenter,
} from "../lib/uiStyles";

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
    <div style={pageCenter}>
      <div style={authCard}>
        <h1 style={authTitle}>Create Account</h1>

        <label style={authLabel}>Username</label>
        <input
          type="text"
          value={username}
          onChange={(e) => setUsername(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && handleRegister()}
          autoComplete="username"
          style={authInput}
        />

        <label style={{ ...authLabel, marginTop: "0.75rem" }}>Password</label>
        <PasswordField
          value={password}
          onChange={setPassword}
          onKeyDown={(e) => e.key === "Enter" && handleRegister()}
          autoComplete="new-password"
        />

        <label style={{ ...authLabel, marginTop: "0.75rem" }}>Confirm Password</label>
        <PasswordField
          value={confirmPassword}
          onChange={setConfirmPassword}
          onKeyDown={(e) => e.key === "Enter" && handleRegister()}
          autoComplete="new-password"
        />

        <AuthSubmitButton onClick={handleRegister} disabled={loading}>
          {loading ? "Creating account…" : "Create account"}
        </AuthSubmitButton>

        <button type="button" onClick={onBack} style={{ ...accentLink, marginTop: "0.75rem" }}>
          ← Back to login
        </button>

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
      </div>
    </div>
  );
}

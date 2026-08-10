import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { useAuth } from "../lib/auth";
import { apiClient, setBaseUrl } from "../lib/api";
import TextField from "../components/TextField";
import PasswordField from "../components/PasswordField";
import AuthSubmitButton from "../components/AuthSubmitButton";
import AuthBackButton from "../components/AuthBackButton";
import {
  authCard,
  authLabel,
  authTitle,
  pageCenter,
} from "../lib/uiStyles";

export default function RegisterScreen() {
  const navigate = useNavigate();
  const { login, serverUrl } = useAuth();
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [confirmPassword, setConfirmPassword] = useState("");
  const [showPassword, setShowPassword] = useState(false);
  const [showConfirm, setShowConfirm] = useState(false);
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
      setBaseUrl(serverUrl.trim());
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
      login(serverUrl.trim(), res.token, res.account_id);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className={pageCenter}>
      <div className={authCard}>
        <h1 className={authTitle}>Create Account</h1>

        <label className={authLabel}>Username</label>
        <TextField
          value={username}
          onChange={setUsername}
          onKeyDown={(e) => e.key === "Enter" && handleRegister()}
          autoComplete="username"
        />

        <label className={`${authLabel} mt-3`}>Password</label>
        <PasswordField
          value={password}
          onChange={setPassword}
          onKeyDown={(e) => e.key === "Enter" && handleRegister()}
          autoComplete="new-password"
          showPassword={showPassword}
          onToggle={() => setShowPassword((v) => !v)}
        />

        <label className={`${authLabel} mt-3`}>Confirm Password</label>
        <PasswordField
          value={confirmPassword}
          onChange={setConfirmPassword}
          onKeyDown={(e) => e.key === "Enter" && handleRegister()}
          autoComplete="new-password"
          showPassword={showConfirm}
          onToggle={() => setShowConfirm((v) => !v)}
        />

        <AuthSubmitButton onClick={handleRegister} disabled={loading}>
          {loading ? "Creating account…" : "Create account"}
        </AuthSubmitButton>

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
          {error || " "}
        </div>

        <AuthBackButton label="Back to login" onClick={() => navigate("/login")} />
      </div>
    </div>
  );
}

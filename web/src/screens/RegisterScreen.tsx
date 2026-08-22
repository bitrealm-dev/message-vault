import { useState } from "react";
import { useNavigate } from "react-router-dom";
import AuthBackButton from "../components/AuthBackButton";
import AuthErrorFooter from "../components/AuthErrorFooter";
import AuthSubmitButton from "../components/AuthSubmitButton";
import PasswordField from "../components/PasswordField";
import TextField from "../components/TextField";
import { apiClient, setBaseUrl } from "../lib/api";
import { useAuth } from "../lib/auth";
import { authCard, authLabel, authTitle, pageCenter } from "../lib/uiStyles";
import { useAsyncAction } from "../lib/useAsyncAction";

export default function RegisterScreen() {
  const navigate = useNavigate();
  const { login, serverUrl } = useAuth();
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [confirmPassword, setConfirmPassword] = useState("");
  const [showPassword, setShowPassword] = useState(false);
  const [showConfirm, setShowConfirm] = useState(false);
  const { busy, error, run } = useAsyncAction();

  const handleRegister = () => {
    void run(async () => {
      if (!username.trim()) {
        throw new Error("Username is required.");
      }
      // Empty passwords are allowed. Only reject when the two fields disagree.
      if (password !== confirmPassword) {
        throw new Error("Passwords do not match.");
      }

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
    });
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

        <AuthSubmitButton onClick={handleRegister} disabled={busy}>
          {busy ? "Creating account…" : "Create account"}
        </AuthSubmitButton>

        <AuthErrorFooter error={error} />

        <AuthBackButton label="Back to login" onClick={() => navigate("/login")} />
      </div>
    </div>
  );
}

import { useState } from "react";
import AuthErrorFooter from "../../components/AuthErrorFooter";
import AuthSubmitButton from "../../components/AuthSubmitButton";
import PasswordField from "../../components/PasswordField";
import TextField from "../../components/TextField";
import { apiClient, setBaseUrl } from "../../lib/api";
import { useAuth } from "../../lib/auth";
import type { SessionResponse } from "../../lib/authGuards";
import { useAsyncAction } from "../../lib/useAsyncAction";

/**
 * New vault account: username plus the password twice.
 *
 * The name and phone numbers are not asked for here — the account is created
 * with an empty profile, which sends the user straight to profile setup.
 */
export default function CreateAccountForm({ serverUrl }: { serverUrl: string }) {
  const { login } = useAuth();
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [confirmPassword, setConfirmPassword] = useState("");
  const [showPassword, setShowPassword] = useState(false);
  const [showConfirm, setShowConfirm] = useState(false);
  const { busy, error, run } = useAsyncAction();

  const submit = () => {
    void run(async () => {
      if (!username.trim()) {
        throw new Error("Username is required.");
      }
      // Only the mismatch is checked here. Length is the server's rule, so it
      // stays there rather than being restated and left to drift.
      if (password !== confirmPassword) {
        throw new Error("Passwords do not match.");
      }

      const url = serverUrl.trim();
      setBaseUrl(url);
      const res = await apiClient.post<SessionResponse>("/v1/auth/register", {
        username: username.trim(),
        password,
        preferred_name: null,
        phone: null,
      });
      // Awaited so the empty-profile check inside `login` runs before this form
      // drops its busy state, sending the new account on to profile setup.
      await login(url, res.token, res.account_id);
    });
  };

  return (
    <>
      <TextField
        label="Username"
        value={username}
        onChange={setUsername}
        onKeyDown={(e) => e.key === "Enter" && submit()}
        autoComplete="username"
      />

      <PasswordField
        label="Password"
        className="mt-3"
        value={password}
        onChange={setPassword}
        onKeyDown={(e) => e.key === "Enter" && submit()}
        autoComplete="new-password"
        showPassword={showPassword}
        onToggle={() => setShowPassword((v) => !v)}
      />
      <p className="mt-1 text-[0.75rem] text-muted">At least 8 characters.</p>

      <PasswordField
        label="Confirm Password"
        className="mt-3"
        value={confirmPassword}
        onChange={setConfirmPassword}
        onKeyDown={(e) => e.key === "Enter" && submit()}
        autoComplete="new-password"
        showPassword={showConfirm}
        onToggle={() => setShowConfirm((v) => !v)}
      />

      <AuthSubmitButton onClick={submit} disabled={busy}>
        {busy ? "Creating account…" : "Create account"}
      </AuthSubmitButton>

      <AuthErrorFooter error={error} />
    </>
  );
}

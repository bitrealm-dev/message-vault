import { useState } from "react";
import AuthErrorFooter from "../../components/AuthErrorFooter";
import AuthSubmitButton from "../../components/AuthSubmitButton";
import PasswordField from "../../components/PasswordField";
import TextField from "../../components/TextField";
import { apiClient } from "../../lib/api";
import { useAuth } from "../../lib/auth";
import type { SessionResponse } from "../../lib/authGuards";
import { useAsyncAction } from "../../lib/useAsyncAction";

/** Username and password sign-in for a vault running in local auth mode. */
export default function LoginForm({ serverUrl }: { serverUrl: string }) {
  const { login } = useAuth();
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [showPassword, setShowPassword] = useState(false);
  const { busy, error, run } = useAsyncAction();

  const submit = () => {
    void run(async () => {
      if (!username.trim()) {
        throw new Error("Username is required.");
      }
      const res = await apiClient.post<SessionResponse>("/v1/auth/login", { username, password });
      // Awaited so the profile lookup inside `login` has decided where to send
      // the user before this form drops its busy state.
      await login(serverUrl.trim(), res.token, res.account_id);
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
        autoComplete="current-password"
        showPassword={showPassword}
        onToggle={() => setShowPassword((v) => !v)}
      />

      <AuthSubmitButton onClick={submit} disabled={busy}>
        {busy ? "Signing in…" : "Sign in"}
      </AuthSubmitButton>

      <AuthErrorFooter error={error} />
    </>
  );
}

import { useState } from "react";
import AuthErrorFooter from "../../components/AuthErrorFooter";
import AuthSubmitButton from "../../components/AuthSubmitButton";
import { LockIcon, PersonIcon } from "../../components/icons";
import PasswordField from "../../components/PasswordField";
import TextField from "../../components/TextField";
import { apiClient, setBaseUrl } from "../../lib/api";
import { useAuth } from "../../lib/auth";
import type { SessionResponse } from "../../lib/authGuards";
import { authCardFooter } from "../../lib/uiStyles";
import { useAsyncAction } from "../../lib/useAsyncAction";

/** Username and password login for a vault running in local auth mode. */
export default function LoginForm({
  serverUrl,
  disabled = false,
}: {
  serverUrl: string;
  disabled?: boolean;
}) {
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
      const url = serverUrl.trim();
      // Re-sync the API client with the address this form is showing, the
      // same as `CreateAccountForm` — `connect()` on the sign-in card can
      // leave the client pointed at a bad host while the form still holds
      // the good address.
      setBaseUrl(url);
      const res = await apiClient.post<SessionResponse>("/v1/auth/login", { username, password });
      // Awaited so the profile lookup inside `login` has decided where to send
      // the user before this form drops its busy state.
      await login(url, res.token, res.account_id);
    });
  };

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <TextField
        label="Username"
        leadingIcon={<PersonIcon size={16} />}
        value={username}
        onChange={setUsername}
        onKeyDown={(e) => e.key === "Enter" && submit()}
        autoComplete="username"
        isDisabled={disabled}
      />

      <PasswordField
        label="Password"
        className="mt-3.5"
        leadingIcon={<LockIcon size={16} />}
        value={password}
        onChange={setPassword}
        onKeyDown={(e) => e.key === "Enter" && submit()}
        autoComplete="current-password"
        showPassword={showPassword}
        onToggle={() => setShowPassword((v) => !v)}
        isDisabled={disabled}
      />

      <div className={authCardFooter}>
        <AuthErrorFooter error={error} />
        <AuthSubmitButton onClick={submit} disabled={busy || disabled}>
          {busy ? "Logging in…" : "Log in"}
        </AuthSubmitButton>
      </div>
    </div>
  );
}

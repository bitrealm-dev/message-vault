import { useState } from "react";
import { useAuth } from "../lib/auth";
import { apiClient } from "../lib/api";
import {
  accentLink,
  authCard,
  authInput,
  authLabel,
  authTitle,
  pageCenter,
} from "../lib/uiStyles";
import AuthSubmitButton from "../components/AuthSubmitButton";
import Select, { ListBoxItem, selectItemClassName } from "../components/Select";
import TextField from "../components/TextField";

interface HandleInput {
  handle: string;
  service: string;
}

const SERVICES: { value: string; label: string }[] = [
  { value: "phone", label: "Phone Number" },
  { value: "email", label: "Email" },
  { value: "whatsapp", label: "WhatsApp" },
];

export default function OnboardingScreen() {
  const { login, logout, token, serverUrl, accountId } = useAuth();
  const [displayName, setDisplayName] = useState("");
  const [handles, setHandles] = useState<HandleInput[]>([
    { handle: "", service: "phone" },
  ]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");

  const addHandle = () => {
    setHandles([...handles, { handle: "", service: "phone" }]);
  };

  const updateHandle = (
    index: number,
    field: keyof HandleInput,
    value: string,
  ) => {
    const next = [...handles];
    next[index] = { ...next[index], [field]: value };
    setHandles(next);
  };

  const removeHandle = (index: number) => {
    if (handles.length === 1) return;
    setHandles(handles.filter((_, i) => i !== index));
  };

  const handleSubmit = async () => {
    setLoading(true);
    setError("");
    try {
      await apiClient.post("/v1/account/profile", {
        preferred_name: displayName.trim(),
        handles: handles
          .filter((h) => h.handle.trim())
          .map((h) => ({
            handle: h.handle.trim(),
            service: h.service,
          })),
      });
      // Re-run login so needsOnboarding is recomputed from the saved profile
      await login(serverUrl, token!, accountId!);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  };

  const canSubmit =
    displayName.trim() && handles.some((h) => h.handle.trim());

  return (
    <div className={pageCenter}>
      <div className={authCard}>
        <h1 className={`${authTitle} mb-2`}>Profile Setup</h1>
        <p className={greetingStyle}>Welcome to the Message Vault!</p>
        <p className={bodyStyle}>
          Set up your profile so we can match imported message data to you.
        </p>

        <label className={authLabel}>Display Name</label>
        <input
          type="text"
          value={displayName}
          onChange={(e) => setDisplayName(e.target.value)}
          placeholder="Your name"
          className={authInput}
          autoFocus
        />

        <label className={`${authLabel} mt-4`}>Source Accounts</label>
        <p className={helpStyle}>
          Add the accounts or phone numbers you import data from.
        </p>

        {handles.map((h, i) => (
          <div
            key={i}
            className="mb-2 flex gap-2"
          >
            <Select
              selectedKey={h.service}
              onSelectionChange={(k) => updateHandle(i, "service", String(k))}
              className="w-[140px] shrink-0"
            >
              {SERVICES.map((s) => (
                <ListBoxItem key={s.value} id={s.value} className={selectItemClassName}>
                  {s.label}
                </ListBoxItem>
              ))}
            </Select>
            <TextField
              value={h.handle}
              onChange={(v) => updateHandle(i, "handle", v)}
              placeholder={
                h.service === "email"
                  ? "you@example.com"
                  : "+1 555-123-4567"
              }
              className="flex-1 min-w-0"
            />
            <button
              type="button"
              onClick={() => removeHandle(i)}
              disabled={handles.length === 1}
              className={`border-none bg-none text-[1.25rem] text-muted ${
                handles.length === 1 ? "cursor-default" : "cursor-pointer"
              }`}
              aria-label="Remove account"
            >
              ×
            </button>
          </div>
        ))}
        <button
          type="button"
          onClick={addHandle}
          className="cursor-pointer border-none bg-none p-0 text-[0.813rem] text-accent"
        >
          + Add another account
        </button>

        <AuthSubmitButton onClick={handleSubmit} disabled={!canSubmit || loading}>
          {loading ? "Saving…" : "Continue to Vault"}
        </AuthSubmitButton>

        <button type="button" onClick={logout} className={`${accentLink} mt-3 block w-full text-center`}>
          ← Back to login
        </button>

        <div
          className="mt-5 min-h-10 text-[0.813rem] leading-[1.35]"
          style={{ color: error ? "var(--danger)" : "transparent" }}
          aria-live="polite"
        >
          {error || " "}
        </div>
      </div>
    </div>
  );
}

const greetingStyle = "text-center text-[0.9375rem] font-medium text-text mb-2";

const bodyStyle = "text-center text-[0.875rem] text-muted mb-6";

const helpStyle = "text-[0.75rem] text-muted mb-2";

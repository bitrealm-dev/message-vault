import { useState } from "react";
import AuthBackButton from "../components/AuthBackButton";
import AuthErrorFooter from "../components/AuthErrorFooter";
import AuthSubmitButton from "../components/AuthSubmitButton";
import Button from "../components/Button";
import Select, { ListBoxItem, selectItemClassName } from "../components/Select";
import TextField from "../components/TextField";
import { apiClient } from "../lib/api";
import { useAuth } from "../lib/auth";
import {
  HANDLE_SERVICE_OPTIONS,
  HANDLE_SERVICES,
  type HandleService,
  handlePlaceholder,
} from "../lib/handleService";
import { parseSelectKey } from "../lib/selectKey";
import { authCard, authCardBody, authCardFooter, authTitle, pageCenter } from "../lib/uiStyles";
import { useAsyncAction } from "../lib/useAsyncAction";

/**
 * The card never scrolls and never resizes, so the list of accounts is bounded.
 * Three covers a number, an address, and one more; longer lists finish in
 * Settings → Profile.
 */
const MAX_ACCOUNT_ROWS = 3;

interface HandleInput {
  id: string;
  handle: string;
  service: HandleService;
}

function newHandleRow(): HandleInput {
  return { id: crypto.randomUUID(), handle: "", service: "phone" };
}

export default function OnboardingScreen() {
  const { login, logout, token, serverUrl, accountId } = useAuth();
  const [displayName, setDisplayName] = useState("");
  const [handles, setHandles] = useState<HandleInput[]>(() => [newHandleRow()]);
  const { busy, error, run } = useAsyncAction();

  const addHandle = () => {
    if (handles.length >= MAX_ACCOUNT_ROWS) return;
    setHandles([...handles, newHandleRow()]);
  };

  const updateHandle = (index: number, field: "handle" | "service", value: string) => {
    const next = [...handles];
    if (field === "service") {
      const service = parseSelectKey(value, HANDLE_SERVICES);
      if (!service) return;
      next[index] = { ...next[index], service };
    } else {
      next[index] = { ...next[index], handle: value };
    }
    setHandles(next);
  };

  const removeHandle = (index: number) => {
    if (handles.length === 1) return;
    setHandles(handles.filter((_, i) => i !== index));
  };

  const handleSubmit = () => {
    void run(async () => {
      if (!token || !accountId) {
        throw new Error("Not signed in");
      }
      await apiClient.post("/v1/account/profile", {
        preferred_name: displayName.trim(),
        handles: handles
          .filter((h) => h.handle.trim())
          .map((h) => ({ handle: h.handle.trim(), service: h.service })),
      });
      // Log in again so "needs setup" is recomputed from the saved profile.
      await login(serverUrl, token, accountId);
    });
  };

  const canSubmit = Boolean(displayName.trim()) && handles.some((h) => h.handle.trim());

  return (
    <div className={pageCenter}>
      <div className={authCard}>
        <div className={authCardBody}>
          <h1 className={`${authTitle} !mb-2`}>Profile Setup</h1>
          <p className="mb-6 text-[0.875rem] text-muted">
            So we can match imported messages to you.
          </p>

          <TextField
            label="Display Name"
            value={displayName}
            onChange={setDisplayName}
            placeholder="Your name"
          />

          <div className="mt-4 mb-2 block text-[0.875rem] font-medium text-text">Your Accounts</div>

          {handles.map((h, i) => (
            <div key={h.id} className="mb-2 flex items-center gap-2">
              <Select
                selectedKey={h.service}
                onSelectionChange={(k) => {
                  const service = parseSelectKey(k, HANDLE_SERVICES);
                  if (service) updateHandle(i, "service", service);
                }}
                className="w-[140px] shrink-0"
                aria-label={`Account ${i + 1} type`}
              >
                {HANDLE_SERVICE_OPTIONS.map((s) => (
                  <ListBoxItem key={s.value} id={s.value} className={selectItemClassName}>
                    {s.label}
                  </ListBoxItem>
                ))}
              </Select>
              <TextField
                value={h.handle}
                onChange={(v) => updateHandle(i, "handle", v)}
                placeholder={handlePlaceholder(h.service)}
                className="min-w-0 flex-1"
                aria-label={`Account ${i + 1} value`}
              />
              {handles.length > 1 ? (
                <Button
                  variant="ghostDanger"
                  size="icon"
                  onPress={() => removeHandle(i)}
                  aria-label={`Remove account ${i + 1}`}
                >
                  ×
                </Button>
              ) : null}
            </div>
          ))}

          {handles.length < MAX_ACCOUNT_ROWS ? (
            <div className="mt-1 flex justify-end">
              <Button variant="secondary" size="sm" onPress={addHandle}>
                + Add account
              </Button>
            </div>
          ) : (
            <p className="mt-1.5 text-right text-[0.75rem] text-muted">
              Add the rest in Settings after setup.
            </p>
          )}
        </div>

        <div className={authCardFooter}>
          <AuthErrorFooter error={error} />
          <AuthSubmitButton onClick={handleSubmit} disabled={!canSubmit || busy}>
            {busy ? "Saving…" : "Continue to Vault"}
          </AuthSubmitButton>
          <AuthBackButton label="Back to Sign In" onClick={logout} />
        </div>
      </div>
    </div>
  );
}

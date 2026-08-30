import { useState } from "react";
import AuthBackButton from "../components/AuthBackButton";
import AuthErrorFooter from "../components/AuthErrorFooter";
import AuthSubmitButton from "../components/AuthSubmitButton";
import Button from "../components/Button";
import { PersonIcon, PhoneIcon } from "../components/icons";
import Select, { ListBoxItem, selectItemClassName } from "../components/Select";
import TextField from "../components/TextField";
import { apiClient } from "../lib/api";
import { useAuth } from "../lib/auth";
import {
  HANDLE_SERVICE_OPTIONS,
  HANDLE_SERVICES,
  type HandleService,
  handlePlaceholder,
  handleValidationError,
} from "../lib/handleService";
import { parseSelectKey } from "../lib/selectKey";
import { authCard, authCardBody, authCardFooter, authTitle, pageCenter } from "../lib/uiStyles";
import { useAsyncAction } from "../lib/useAsyncAction";

/**
 * The card never scrolls and never resizes, so the list of accounts is bounded
 * by what fits inside the frame. Five rows fill the space the card has;
 * anyone with more finishes the list in Settings → Profile.
 */
const MAX_ACCOUNT_ROWS = 5;

interface HandleInput {
  id: string;
  handle: string;
  service: HandleService;
}

function newHandleRow(): HandleInput {
  return { id: crypto.randomUUID(), handle: "", service: "phone" };
}

/**
 * A handset for the services reached by phone number, a person for the ones
 * reached by address. The glyph says what kind of thing the field wants before
 * the placeholder does, so it is set slightly larger than the icons that only
 * decorate a label.
 */
function serviceIcon(service: HandleService) {
  return service === "email" ? <PersonIcon size={18} /> : <PhoneIcon size={18} />;
}

export default function OnboardingScreen() {
  const { login, logout, token, serverUrl, accountId } = useAuth();
  const [displayName, setDisplayName] = useState("");
  const [handles, setHandles] = useState<HandleInput[]>(() => [newHandleRow()]);
  // Rows whose value does not read as the kind of account it is set to. Held
  // by id rather than index so removing a row cannot move the mark onto a
  // different one.
  const [invalidIds, setInvalidIds] = useState<string[]>([]);
  const [validationError, setValidationError] = useState("");
  const { busy, error, run } = useAsyncAction();

  /**
   * Check every filled-in row, mark the ones that do not read as a usable
   * account, and report the first reason. Returns whether the list is usable.
   * Empty rows are left alone — they are rows not filled in yet, not mistakes.
   */
  const revalidate = (rows: HandleInput[]) => {
    const failures = rows.filter((row) => handleValidationError(row.service, row.handle));
    setInvalidIds(failures.map((row) => row.id));
    setValidationError(
      failures.length ? (handleValidationError(failures[0].service, failures[0].handle) ?? "") : "",
    );
    return failures.length === 0;
  };

  const addHandle = () => {
    if (handles.length >= MAX_ACCOUNT_ROWS) return;
    // A new empty row while one above it is wrong just buries the mistake, so
    // the list does not grow until what is already in it holds up.
    if (!revalidate(handles)) return;
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

    // A row being edited stops reading as wrong straight away rather than
    // staying red under the cursor; leaving the field checks it again.
    const remaining = invalidIds.filter((id) => id !== next[index].id);
    if (remaining.length !== invalidIds.length) setInvalidIds(remaining);
    if (remaining.length === 0) setValidationError("");
  };

  const removeHandle = (index: number) => {
    if (handles.length === 1) return;
    // Removing a row is itself a way to fix the list, so the row goes first and
    // what is left is judged after.
    const next = handles.filter((_, i) => i !== index);
    setHandles(next);
    revalidate(next);
  };

  const handleSubmit = () => {
    if (!revalidate(handles)) return;
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
          <p className="mt-0 mb-4 text-[0.875rem] text-muted">
            So we can match imported messages to you.
          </p>

          <TextField
            label="Display Name"
            value={displayName}
            onChange={setDisplayName}
            placeholder="Your name"
          />

          <div className="mt-4 mb-2 block text-[0.875rem] font-medium text-text">Your Accounts</div>

          {handles.map((h, i) => {
            const invalid = invalidIds.includes(h.id);
            return (
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
                  onBlur={() => revalidate(handles)}
                  leadingIcon={serviceIcon(h.service)}
                  placeholder={handlePlaceholder(h.service)}
                  className="min-w-0 flex-1"
                  inputClassName={invalid ? "!border-danger" : undefined}
                  isInvalid={invalid}
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
            );
          })}

          {handles.length < MAX_ACCOUNT_ROWS ? (
            <div className="mt-3 flex justify-end">
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
          {/* A value that does not read as an account is reported in the same
              place as anything the server sends back, so there is one line on
              this card that carries what is wrong. */}
          <AuthErrorFooter error={validationError || error} />
          {/* One row: the way back on the left, the way on at half width on
              the right, matching the button on the card before this one. */}
          <div className="mt-6 flex items-center justify-between gap-3">
            <AuthBackButton label="Back to login" onClick={logout} />
            <AuthSubmitButton
              onClick={handleSubmit}
              disabled={!canSubmit || busy}
              className="w-1/2"
            >
              {busy ? "Saving…" : "Continue to vault"}
            </AuthSubmitButton>
          </div>
        </div>
      </div>
    </div>
  );
}

import { useEffect, useState } from "react";
import Button from "../../components/Button";
import Select, { ListBoxItem, selectItemClassName } from "../../components/Select";
import type { AccountProfile } from "../../lib/account";
import {
  HANDLE_SERVICE_OPTIONS,
  HANDLE_SERVICES,
  type HandleService,
  handlePlaceholder,
} from "../../lib/handleService";
import { phonesMatch } from "../../lib/phoneTokens";
import { parseSelectKey } from "../../lib/selectKey";
import { useAccountProfile } from "../../lib/useAccountProfile";
import { updateAccountProfile } from "../../lib/vaultApi";
import { inputClassName, sectionTitleClass } from "./profileStyles";

/** Profile settings: display name and phone/email/WhatsApp handles. */
export function ProfileSettingsPanel() {
  const { profile, setProfile, loading, error: loadError } = useAccountProfile();
  const [name, setName] = useState("");
  const [nameSaved, setNameSaved] = useState(false);
  const [nameError, setNameError] = useState("");

  const [newHandle, setNewHandle] = useState("");
  const [newHandleService, setNewHandleService] = useState<HandleService>("phone");
  const [handleError, setHandleError] = useState("");
  const [handleBusy, setHandleBusy] = useState(false);

  useEffect(() => {
    if (profile) setName(profile.preferred_name ?? "");
  }, [profile]);

  if (loadError) {
    return <div className="text-danger">Could not load profile: {loadError}</div>;
  }

  if (loading || !profile) {
    return <div className="text-muted">Loading…</div>;
  }

  const handleSaveName = async () => {
    setNameError("");
    try {
      const updated = await updateAccountProfile({
        preferred_name: name.trim() || null,
      });
      setProfile(updated);
      setName(updated.preferred_name ?? "");
      setNameSaved(true);
      setTimeout(() => setNameSaved(false), 2000);
    } catch (e) {
      setNameError(e instanceof Error ? e.message : String(e));
    }
  };

  const handleListIncludes = (p: AccountProfile, handle: string, service: string) => {
    const needle = handle.trim().toLowerCase();
    if (service === "email") {
      return p.emails.some((e) => e.toLowerCase() === needle);
    }
    // Phone and WhatsApp both come back in profile.phones (E.164 when unambiguous).
    return p.phones.some((phone) => phonesMatch(handle, phone));
  };

  const handleAddHandle = async () => {
    const value = newHandle.trim();
    if (!value) return;
    setHandleError("");
    setHandleBusy(true);
    try {
      const updated = await updateAccountProfile({
        handles: [{ handle: value, service: newHandleService }],
      });
      setProfile(updated);
      if (!handleListIncludes(updated, value, newHandleService)) {
        throw new Error("The vault did not add that handle.");
      }
      setNewHandle("");
    } catch (e) {
      setHandleError(e instanceof Error ? e.message : String(e));
    } finally {
      setHandleBusy(false);
    }
  };

  const handleRemoveHandle = async (handle: string, service: string) => {
    setHandleError("");
    setHandleBusy(true);
    try {
      const updated = await updateAccountProfile({
        remove_handles: [{ handle, service }],
      });
      setProfile(updated);
      if (handleListIncludes(updated, handle, service)) {
        throw new Error("The vault did not remove that handle.");
      }
    } catch (e) {
      setHandleError(e instanceof Error ? e.message : String(e));
    } finally {
      setHandleBusy(false);
    }
  };

  const handles = [
    ...profile.phones.map((handle) => ({ handle, service: "phone" })),
    ...profile.emails.map((handle) => ({ handle, service: "email" })),
  ];

  return (
    <div>
      <h3 className={sectionTitleClass}>Display Name</h3>
      <div className="mb-[0.35rem] flex gap-2">
        <input
          type="text"
          value={name}
          onChange={(e) => setName(e.target.value)}
          className={`${inputClassName} flex-1`}
        />
        <Button variant="primary" onClick={handleSaveName} className="!px-4 !py-1">
          {nameSaved ? "Saved" : "Save"}
        </Button>
      </div>
      {nameError && <div className="mb-6 text-[0.813rem] text-danger">{nameError}</div>}
      {!nameError && <div className="mb-6" />}

      <h3 className={sectionTitleClass}>My Handles</h3>
      {handles.length === 0 ? (
        <div className="mb-3 text-[0.875rem] text-muted">
          No phone or email handles on this account yet.
        </div>
      ) : (
        <div className="mb-3">
          {handles.map((h) => (
            <div
              key={`${h.service}-${h.handle}`}
              className="flex items-center gap-3 border-b border-border py-1.5 text-[0.875rem]"
            >
              <span className="min-w-[7rem] shrink-0 text-muted">{h.service}</span>
              <span className="min-w-0 flex-1">{h.handle}</span>
              <Button
                variant="ghost"
                onClick={() => handleRemoveHandle(h.handle, h.service)}
                disabled={handleBusy}
                className="!px-2 !py-[0.2rem] !text-[0.813rem] !text-danger"
              >
                Remove
              </Button>
            </div>
          ))}
        </div>
      )}

      <div className="mb-[0.35rem] flex flex-wrap items-center gap-2">
        <Select
          selectedKey={newHandleService}
          onSelectionChange={(k) => {
            const service = parseSelectKey(k, HANDLE_SERVICES);
            if (service) setNewHandleService(service);
          }}
          aria-label="Handle service"
          className="shrink-0 min-w-[7rem]"
        >
          {HANDLE_SERVICE_OPTIONS.map((s) => (
            <ListBoxItem key={s.value} id={s.value} className={selectItemClassName}>
              {s.label}
            </ListBoxItem>
          ))}
        </Select>
        <input
          type="text"
          value={newHandle}
          onChange={(e) => setNewHandle(e.target.value)}
          placeholder={handlePlaceholder(newHandleService)}
          className={`${inputClassName} min-w-[12rem] flex-1`}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              e.preventDefault();
              void handleAddHandle();
            }
          }}
        />
        <Button
          variant="primary"
          onClick={handleAddHandle}
          disabled={handleBusy || !newHandle.trim()}
          className="!px-[0.85rem] !py-[0.35rem]"
        >
          Add
        </Button>
      </div>
      {handleError && <div className="mb-6 text-[0.813rem] text-danger">{handleError}</div>}
    </div>
  );
}

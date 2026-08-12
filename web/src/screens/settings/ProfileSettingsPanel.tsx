import { useState, useEffect } from "react";
import { apiClient } from "../../lib/api";
import Button from "../../components/Button";
import Select, { ListBoxItem, selectItemClassName } from "../../components/Select";
import { parseSelectKey } from "../../lib/selectKey";
import {
  type AccountProfile,
  inputClassName,
  sectionTitleClass,
} from "./profileStyles";

/** Profile settings: display name and phone/email/WhatsApp handles. */
export function ProfileSettingsPanel() {
  const [profile, setProfile] = useState<AccountProfile | null>(null);
  const [loadError, setLoadError] = useState("");
  const [name, setName] = useState("");
  const [nameSaved, setNameSaved] = useState(false);
  const [nameError, setNameError] = useState("");

  const [newHandle, setNewHandle] = useState("");
  const [newHandleService, setNewHandleService] = useState<"phone" | "email" | "whatsapp">("phone");
  const [handleError, setHandleError] = useState("");
  const [handleBusy, setHandleBusy] = useState(false);

  useEffect(() => {
    apiClient
      .get<AccountProfile>("/v1/account/profile")
      .then((p) => {
        setProfile(p);
        setName(p.preferred_name ?? "");
      })
      .catch((e) => setLoadError(e instanceof Error ? e.message : String(e)));
  }, []);

  if (loadError) {
    return (
      <div className="text-danger">
        Could not load profile: {loadError}
      </div>
    );
  }

  if (!profile) {
    return <div className="text-muted">Loading…</div>;
  }

  const handleSaveName = async () => {
    setNameError("");
    try {
      const updated = await apiClient.post<AccountProfile>("/v1/account/profile", {
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
    // Phones/WhatsApp are E.164-normalized server-side; match by digit suffix.
    const digits = handle.replace(/\D/g, "");
    return p.phones.some((phone) => {
      const phoneDigits = phone.replace(/\D/g, "");
      return phoneDigits === digits || phone.toLowerCase() === needle;
    });
  };

  const handleAddHandle = async () => {
    const value = newHandle.trim();
    if (!value) return;
    setHandleError("");
    setHandleBusy(true);
    try {
      const updated = await apiClient.post<AccountProfile>("/v1/account/profile", {
        handles: [{ handle: value, service: newHandleService }],
      });
      if (!handleListIncludes(updated, value, newHandleService)) {
        throw new Error(
          "Server did not save the handle. Restart the vault server (docker compose restart vault) and try again.",
        );
      }
      setProfile(updated);
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
      const updated = await apiClient.post<AccountProfile>("/v1/account/profile", {
        remove_handles: [{ handle, service }],
      });
      if (handleListIncludes(updated, handle, service)) {
        throw new Error(
          "Server did not remove the handle. Restart the vault server (docker compose restart vault) and try again.",
        );
      }
      setProfile(updated);
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
        <Button
          variant="primary"
          onClick={handleSaveName}
          className="!px-4 !py-1"
        >
          {nameSaved ? "Saved" : "Save"}
        </Button>
      </div>
      {nameError && (
        <div className="mb-6 text-[0.813rem] text-danger">
          {nameError}
        </div>
      )}
      {!nameError && <div className="mb-6" />}

      <h3 className={sectionTitleClass}>My Handles</h3>
      {handles.length === 0 ? (
        <div className="mb-3 text-[0.875rem] text-muted">
          No phone or email handles on this account yet.
        </div>
      ) : (
        <div className="mb-3">
          {handles.map((h, i) => (
            <div
              key={`${h.service}-${h.handle}-${i}`}
              className="flex items-center gap-3 border-b border-border py-1.5 text-[0.875rem]"
            >
              <span className="min-w-[7rem] shrink-0 text-muted">
                {h.service}
              </span>
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
            const service = parseSelectKey(k, ["phone", "email", "whatsapp"] as const);
            if (service) setNewHandleService(service);
          }}
          aria-label="Handle service"
          className="shrink-0 min-w-[7rem]"
        >
          <ListBoxItem id="phone" className={selectItemClassName}>Phone</ListBoxItem>
          <ListBoxItem id="email" className={selectItemClassName}>Email</ListBoxItem>
          <ListBoxItem id="whatsapp" className={selectItemClassName}>WhatsApp</ListBoxItem>
        </Select>
        <input
          type="text"
          value={newHandle}
          onChange={(e) => setNewHandle(e.target.value)}
          placeholder={
            newHandleService === "email" ? "name@example.com" : "+1 555 555 0100"
          }
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
      {handleError && (
        <div className="mb-6 text-[0.813rem] text-danger">
          {handleError}
        </div>
      )}
    </div>
  );
}

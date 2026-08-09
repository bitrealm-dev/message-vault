import { useState, useEffect } from "react";
import { useAuth } from "../../lib/auth";
import { apiClient } from "../../lib/api";
import Button from "../../components/Button";
import { ProfileDangerZone } from "./ProfileDangerZone";
import {
  type AccountProfile,
  inputStyle,
  sectionTitle,
} from "./profileStyles";

/** Profile settings body for the Settings Profile tab (no page chrome). */
export function ProfileSettingsPanel() {
  const { token } = useAuth();
  const [profile, setProfile] = useState<AccountProfile | null>(null);
  const [loadError, setLoadError] = useState("");
  const [name, setName] = useState("");
  const [nameSaved, setNameSaved] = useState(false);
  const [nameError, setNameError] = useState("");

  const [currentPw, setCurrentPw] = useState("");
  const [newPw, setNewPw] = useState("");
  const [confirmPw, setConfirmPw] = useState("");
  const [pwMsg, setPwMsg] = useState("");
  const [pwOk, setPwOk] = useState(false);

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
      <div style={{ color: "var(--danger)" }}>
        Could not load profile: {loadError}
      </div>
    );
  }

  if (!profile) {
    return <div style={{ color: "var(--muted)" }}>Loading…</div>;
  }

  const isDemo = profile.is_demo === true;

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

  const handleChangePassword = async () => {
    setPwMsg("");
    setPwOk(false);
    if (newPw.length < 8) {
      setPwMsg("New password must be at least 8 characters.");
      return;
    }
    if (newPw !== confirmPw) {
      setPwMsg("New password and confirmation do not match.");
      return;
    }
    try {
      await apiClient.post("/v1/auth/change-password", {
        current_password: currentPw,
        new_password: newPw,
      });
      setPwOk(true);
      setPwMsg("Password changed.");
      setCurrentPw("");
      setNewPw("");
      setConfirmPw("");
    } catch (e) {
      setPwMsg(e instanceof Error ? e.message : String(e));
    }
  };

  const handles = [
    ...profile.phones.map((handle) => ({ handle, service: "phone" })),
    ...profile.emails.map((handle) => ({ handle, service: "email" })),
  ];

  return (
    <div>
      <h3 style={sectionTitle}>Username</h3>
      <input type="text" value={profile.username} readOnly style={{ ...inputStyle, marginBottom: "1.5rem", backgroundColor: "var(--elevated)", color: "var(--muted)" }} />

      <h3 style={sectionTitle}>Display Name</h3>
      <div style={{ display: "flex", gap: "0.5rem", marginBottom: "0.35rem" }}>
        <input
          type="text"
          value={name}
          onChange={(e) => setName(e.target.value)}
          style={{ ...inputStyle, flex: 1 }}
        />
        <Button
          variant="primary"
          onClick={handleSaveName}
          style={{ padding: "0.25rem 1rem" }}
        >
          {nameSaved ? "Saved" : "Save"}
        </Button>
      </div>
      {nameError && (
        <div style={{ fontSize: "0.813rem", color: "var(--danger)", marginBottom: "1.5rem" }}>
          {nameError}
        </div>
      )}
      {!nameError && <div style={{ marginBottom: "1.5rem" }} />}

      <h3 style={sectionTitle}>My Handles</h3>
      {handles.length === 0 ? (
        <div style={{ fontSize: "0.875rem", color: "var(--muted)", marginBottom: "0.75rem" }}>
          No phone or email handles on this account yet.
        </div>
      ) : (
        <div style={{ marginBottom: "0.75rem" }}>
          {handles.map((h, i) => (
            <div
              key={`${h.service}-${h.handle}-${i}`}
              style={{
                display: "flex",
                alignItems: "center",
                gap: "0.75rem",
                padding: "0.375rem 0",
                borderBottom: "1px solid var(--border)",
                fontSize: "0.875rem",
              }}
            >
              <span style={{ color: "var(--muted)", minWidth: "7rem", flexShrink: 0 }}>
                {h.service}
              </span>
              <span style={{ flex: 1, minWidth: 0 }}>{h.handle}</span>
              <Button
                variant="ghost"
                onClick={() => handleRemoveHandle(h.handle, h.service)}
                disabled={handleBusy}
                style={{
                  fontSize: "0.813rem",
                  padding: "0.2rem 0.5rem",
                  color: "var(--danger)",
                }}
              >
                Remove
              </Button>
            </div>
          ))}
        </div>
      )}

      <div
        style={{
          display: "flex",
          gap: "0.5rem",
          flexWrap: "wrap",
          marginBottom: "0.35rem",
          alignItems: "center",
        }}
      >
        <select
          value={newHandleService}
          onChange={(e) => setNewHandleService(e.target.value as "phone" | "email" | "whatsapp")}
          style={{ ...inputStyle, width: "auto", minWidth: "7rem" }}
        >
          <option value="phone">Phone</option>
          <option value="email">Email</option>
          <option value="whatsapp">WhatsApp</option>
        </select>
        <input
          type="text"
          value={newHandle}
          onChange={(e) => setNewHandle(e.target.value)}
          placeholder={
            newHandleService === "email" ? "name@example.com" : "+1 555 555 0100"
          }
          style={{ ...inputStyle, flex: 1, minWidth: "12rem" }}
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
          style={{ padding: "0.35rem 0.85rem" }}
        >
          Add
        </Button>
      </div>
      {handleError && (
        <div style={{ fontSize: "0.813rem", color: "var(--danger)", marginBottom: "1.5rem" }}>
          {handleError}
        </div>
      )}
      {!handleError && <div style={{ marginBottom: "1.5rem" }} />}

      <h3 style={sectionTitle}>Message import</h3>
      <p style={{ margin: "0 0 0.5rem", fontSize: "0.813rem", color: "var(--muted)" }}>
        API token used for Import and Push into this vault.
      </p>
      {token ? (
        <div
          style={{
            ...inputStyle,
            marginBottom: "1.5rem",
            backgroundColor: "var(--elevated)",
            color: "var(--text)",
            fontFamily: "ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace",
            fontSize: "0.813rem",
            wordBreak: "break-all",
            whiteSpace: "pre-wrap",
          }}
        >
          {token}
        </div>
      ) : (
        <div style={{ fontSize: "0.813rem", color: "var(--muted)", marginBottom: "1.5rem" }}>
          Sign in again to see token
        </div>
      )}

      <h3 style={sectionTitle}>Change Password</h3>
      <div style={{ marginBottom: "1.5rem", maxWidth: "360px" }}>
        <label style={{ fontSize: "0.813rem", fontWeight: 500, display: "block", marginBottom: "0.25rem" }}>
          Current password
        </label>
        <input
          type="password"
          value={currentPw}
          onChange={(e) => setCurrentPw(e.target.value)}
          autoComplete="current-password"
          style={{ ...inputStyle, marginBottom: "0.5rem" }}
        />
        <label style={{ fontSize: "0.813rem", fontWeight: 500, display: "block", marginBottom: "0.25rem" }}>
          New password
        </label>
        <input
          type="password"
          value={newPw}
          onChange={(e) => setNewPw(e.target.value)}
          autoComplete="new-password"
          style={{ ...inputStyle, marginBottom: "0.5rem" }}
        />
        <label style={{ fontSize: "0.813rem", fontWeight: 500, display: "block", marginBottom: "0.25rem" }}>
          Confirm new password
        </label>
        <input
          type="password"
          value={confirmPw}
          onChange={(e) => setConfirmPw(e.target.value)}
          autoComplete="new-password"
          style={{ ...inputStyle, marginBottom: "0.5rem" }}
        />
        <Button
          variant="primary"
          onClick={handleChangePassword}
          disabled={!currentPw || !newPw || !confirmPw}
          style={{ padding: "0.375rem 0.75rem" }}
        >
          Change password
        </Button>
        {pwMsg && (
          <div
            style={{
              marginTop: "0.375rem",
              fontSize: "0.813rem",
              color: pwOk ? "var(--ok)" : "var(--danger)",
            }}
          >
            {pwMsg}
          </div>
        )}
      </div>

      <ProfileDangerZone isDemo={isDemo} username={profile.username} />
    </div>
  );
}

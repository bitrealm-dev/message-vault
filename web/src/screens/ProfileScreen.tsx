import { useState, useEffect } from "react";
import { useAuth } from "../lib/auth";
import { apiClient } from "../lib/api";
import DeleteAccountDialog from "../components/DeleteAccountDialog";

interface AccountProfile {
  account_id: string;
  username: string;
  preferred_name: string | null;
  phones: string[];
  emails: string[];
  is_demo?: boolean;
  read_only?: boolean;
}

interface StorageStats {
  conversations: number;
  messages: number;
  attachments: number;
}

const inputStyle: React.CSSProperties = {
  width: "100%",
  padding: "0.35rem 0.5rem",
  fontSize: "0.875rem",
  border: "1px solid #d1d5db",
  borderRadius: "4px",
  boxSizing: "border-box",
};

const sectionTitle: React.CSSProperties = {
  fontSize: "0.875rem",
  color: "#6b7280",
  margin: "0 0 0.5rem",
};

const dangerButtonStyle: React.CSSProperties = {
  width: "10rem",
  flexShrink: 0,
  padding: "0.5rem 0.75rem",
  fontSize: "0.813rem",
  color: "#dc2626",
  border: "1px solid #fecaca",
  background: "#fef2f2",
  borderRadius: "4px",
  cursor: "pointer",
};

export default function ProfileScreen() {
  const { logout } = useAuth();
  const [profile, setProfile] = useState<AccountProfile | null>(null);
  const [loadError, setLoadError] = useState("");
  const [name, setName] = useState("");
  const [nameSaved, setNameSaved] = useState(false);
  const [nameError, setNameError] = useState("");
  const [storage, setStorage] = useState<StorageStats | null>(null);

  const [currentPw, setCurrentPw] = useState("");
  const [newPw, setNewPw] = useState("");
  const [confirmPw, setConfirmPw] = useState("");
  const [pwMsg, setPwMsg] = useState("");
  const [pwOk, setPwOk] = useState(false);

  const [newHandle, setNewHandle] = useState("");
  const [newHandleService, setNewHandleService] = useState<"phone" | "email" | "whatsapp">("phone");
  const [handleError, setHandleError] = useState("");
  const [handleBusy, setHandleBusy] = useState(false);

  const [dangerZoneOpen, setDangerZoneOpen] = useState(false);
  const [deleteDialogOpen, setDeleteDialogOpen] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [deletingMessages, setDeletingMessages] = useState(false);
  const [dangerError, setDangerError] = useState("");

  const refreshStorage = () => {
    apiClient
      .get<StorageStats>("/v1/export/messages/count?q=")
      .then(setStorage)
      .catch(() => {});
  };

  useEffect(() => {
    apiClient
      .get<AccountProfile>("/v1/account/profile")
      .then((p) => {
        setProfile(p);
        setName(p.preferred_name ?? "");
      })
      .catch((e) => setLoadError(e instanceof Error ? e.message : String(e)));

    refreshStorage();
  }, []);

  if (loadError) {
    return (
      <div style={{ padding: "1.5rem", color: "#dc2626" }}>
        Could not load profile: {loadError}
      </div>
    );
  }

  if (!profile) {
    return <div style={{ padding: "1.5rem", color: "#9ca3af" }}>Loading…</div>;
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

  const deleteAllMessages = async () => {
    if (
      !confirm(
        "Delete all messages and attachments? Your contacts and settings will remain.",
      )
    ) {
      return;
    }
    setDeletingMessages(true);
    setDangerError("");
    try {
      await apiClient.post("/v1/account/delete-messages", { confirm: true });
      refreshStorage();
    } catch (e) {
      setDangerError(e instanceof Error ? e.message : String(e));
    } finally {
      setDeletingMessages(false);
    }
  };

  const performDeleteAccount = async () => {
    setDeleting(true);
    setDangerError("");
    try {
      await apiClient.post("/v1/auth/delete-account", { confirm: true });
      setDeleteDialogOpen(false);
      logout();
    } catch (e) {
      setDangerError(e instanceof Error ? e.message : String(e));
    } finally {
      setDeleting(false);
    }
  };

  const handles = [
    ...profile.phones.map((handle) => ({ handle, service: "phone" })),
    ...profile.emails.map((handle) => ({ handle, service: "email" })),
  ];

  const busy = deleting || deletingMessages;

  return (
    <div style={{ padding: "1.5rem", maxWidth: "700px" }}>
      <h2 style={{ margin: "0 0 1.5rem 0" }}>My Profile</h2>

      <h3 style={sectionTitle}>Username</h3>
      <input type="text" value={profile.username} readOnly style={{ ...inputStyle, marginBottom: "1.5rem", background: "#f9fafb", color: "#6b7280" }} />

      <h3 style={sectionTitle}>Display Name</h3>
      <div style={{ display: "flex", gap: "0.5rem", marginBottom: "0.35rem" }}>
        <input
          type="text"
          value={name}
          onChange={(e) => setName(e.target.value)}
          style={{ ...inputStyle, flex: 1 }}
        />
        <button
          type="button"
          onClick={handleSaveName}
          style={{ padding: "0.25rem 1rem", fontWeight: 600 }}
        >
          {nameSaved ? "Saved" : "Save"}
        </button>
      </div>
      {nameError && (
        <div style={{ fontSize: "0.813rem", color: "#dc2626", marginBottom: "1.5rem" }}>
          {nameError}
        </div>
      )}
      {!nameError && <div style={{ marginBottom: "1.5rem" }} />}

      <h3 style={sectionTitle}>My Handles</h3>
      {handles.length === 0 ? (
        <div style={{ fontSize: "0.875rem", color: "#9ca3af", marginBottom: "0.75rem" }}>
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
                borderBottom: "1px solid #f3f4f6",
                fontSize: "0.875rem",
              }}
            >
              <span style={{ flex: 1 }}>{h.handle}</span>
              <span style={{ color: "#6b7280", minWidth: "3.5rem" }}>{h.service}</span>
              <button
                type="button"
                onClick={() => handleRemoveHandle(h.handle, h.service)}
                disabled={handleBusy}
                style={{
                  fontSize: "0.813rem",
                  color: "#dc2626",
                  border: "1px solid #fecaca",
                  background: "#fff",
                  borderRadius: "4px",
                  padding: "0.2rem 0.5rem",
                  cursor: handleBusy ? "not-allowed" : "pointer",
                }}
              >
                Remove
              </button>
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
        <button
          type="button"
          onClick={handleAddHandle}
          disabled={handleBusy || !newHandle.trim()}
          style={{ padding: "0.35rem 0.85rem", fontWeight: 600 }}
        >
          Add
        </button>
      </div>
      {handleError && (
        <div style={{ fontSize: "0.813rem", color: "#dc2626", marginBottom: "1.5rem" }}>
          {handleError}
        </div>
      )}
      {!handleError && <div style={{ marginBottom: "1.5rem" }} />}

      <h3 style={sectionTitle}>Storage</h3>
      <div style={{ fontSize: "0.875rem", color: "#374151", marginBottom: "1.5rem" }}>
        {storage ? (
          <>
            <div>{storage.messages.toLocaleString()} messages</div>
            <div>{storage.attachments.toLocaleString()} attachments</div>
            <div>{storage.conversations.toLocaleString()} conversations</div>
          </>
        ) : (
          <div style={{ color: "#9ca3af" }}>Loading…</div>
        )}
      </div>

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
        <button
          type="button"
          onClick={handleChangePassword}
          disabled={!currentPw || !newPw || !confirmPw}
          style={{ padding: "0.375rem 0.75rem", fontSize: "0.875rem" }}
        >
          Change password
        </button>
        {pwMsg && (
          <div
            style={{
              marginTop: "0.375rem",
              fontSize: "0.813rem",
              color: pwOk ? "#16a34a" : "#dc2626",
            }}
          >
            {pwMsg}
          </div>
        )}
      </div>

      <section style={{ marginTop: "2rem", paddingTop: "1.5rem", borderTop: "1px solid #e5e7eb" }}>
        <button
          type="button"
          aria-expanded={dangerZoneOpen}
          onClick={() => setDangerZoneOpen((open) => !open)}
          style={{
            display: "flex",
            alignItems: "center",
            gap: "0.5rem",
            width: "100%",
            padding: 0,
            border: "none",
            background: "transparent",
            cursor: "pointer",
            textAlign: "left",
          }}
        >
          <span
            style={{
              display: "inline-block",
              transform: dangerZoneOpen ? "rotate(90deg)" : "none",
              transition: "transform 0.15s ease",
              color: "#dc2626",
              fontSize: "0.75rem",
            }}
          >
            ▶
          </span>
          <span
            style={{
              fontSize: "0.75rem",
              fontWeight: 600,
              letterSpacing: "0.06em",
              textTransform: "uppercase",
              color: "#dc2626",
            }}
          >
            Danger zone
          </span>
        </button>
        <p style={{ margin: "0.35rem 0 0 1.25rem", fontSize: "0.813rem", color: "#6b7280" }}>
          Delete messages or permanently remove your account.
        </p>

        {dangerZoneOpen && (
          <div style={{ marginTop: "1rem", marginLeft: "1.25rem", display: "flex", flexDirection: "column", gap: "1rem" }}>
            <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", gap: "1rem" }}>
              <p style={{ margin: 0, flex: 1, fontSize: "0.813rem", color: "#6b7280" }}>
                Delete all messages and attachments. Your contacts and settings remain.
              </p>
              <button
                type="button"
                disabled={busy}
                onClick={() => void deleteAllMessages()}
                style={{
                  ...dangerButtonStyle,
                  opacity: busy ? 0.5 : 1,
                  cursor: busy ? "not-allowed" : "pointer",
                }}
              >
                {deletingMessages ? "Deleting…" : "Delete all messages"}
              </button>
            </div>

            {!isDemo && (
              <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", gap: "1rem" }}>
                <p style={{ margin: 0, flex: 1, fontSize: "0.813rem", color: "#6b7280" }}>
                  Delete this account and everything in it.
                </p>
                <button
                  type="button"
                  disabled={busy}
                  onClick={() => setDeleteDialogOpen(true)}
                  style={{
                    ...dangerButtonStyle,
                    opacity: busy ? 0.5 : 1,
                    cursor: busy ? "not-allowed" : "pointer",
                  }}
                >
                  Delete account
                </button>
              </div>
            )}

            {dangerError && (
              <div style={{ fontSize: "0.813rem", color: "#dc2626" }} role="alert">
                {dangerError}
              </div>
            )}
          </div>
        )}
      </section>

      <DeleteAccountDialog
        open={deleteDialogOpen}
        username={profile.username}
        deleting={deleting}
        onClose={() => {
          if (!deleting) setDeleteDialogOpen(false);
        }}
        onConfirm={() => void performDeleteAccount()}
      />
    </div>
  );
}

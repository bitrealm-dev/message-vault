import { useState, useEffect } from "react";
import { apiClient } from "../../lib/api";
import Button from "../../components/Button";
import { ProfileDangerZone } from "./ProfileDangerZone";
import { ApiTokensSection } from "./ApiTokensSection";
import {
  type AccountProfile,
  inputClassName,
  sectionTitleClass,
} from "./profileStyles";

/** Account settings: username, password, API tokens, danger zone. */
export function AccountSettingsPanel() {
  const [profile, setProfile] = useState<AccountProfile | null>(null);
  const [loadError, setLoadError] = useState("");

  const [currentPw, setCurrentPw] = useState("");
  const [newPw, setNewPw] = useState("");
  const [confirmPw, setConfirmPw] = useState("");
  const [pwMsg, setPwMsg] = useState("");
  const [pwOk, setPwOk] = useState(false);

  useEffect(() => {
    apiClient
      .get<AccountProfile>("/v1/account/profile")
      .then(setProfile)
      .catch((e) => setLoadError(e instanceof Error ? e.message : String(e)));
  }, []);

  if (loadError) {
    return (
      <div style={{ color: "var(--danger)" }}>
        Could not load account: {loadError}
      </div>
    );
  }

  if (!profile) {
    return <div style={{ color: "var(--muted)" }}>Loading…</div>;
  }

  const isDemo = profile.is_demo === true;

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

  return (
    <div>
      <h3 className={sectionTitleClass}>Username</h3>
      <div style={{ maxWidth: "360px", marginBottom: "1.5rem" }}>
        <input
          type="text"
          value={profile.username}
          readOnly
          className={inputClassName}
          style={{
            backgroundColor: "var(--elevated)",
            color: "var(--muted)",
          }}
        />
      </div>
      <h3 className={sectionTitleClass}>Change Password</h3>
      <div style={{ marginBottom: "1.5rem", maxWidth: "360px" }}>
        <label style={{ fontSize: "0.813rem", fontWeight: 500, display: "block", marginBottom: "0.25rem" }}>
          Current password
        </label>
        <input
          type="password"
          value={currentPw}
          onChange={(e) => setCurrentPw(e.target.value)}
          autoComplete="current-password"
          disabled={isDemo}
          className={inputClassName}
          style={{ marginBottom: "0.5rem" }}
        />
        <label style={{ fontSize: "0.813rem", fontWeight: 500, display: "block", marginBottom: "0.25rem" }}>
          New password
        </label>
        <input
          type="password"
          value={newPw}
          onChange={(e) => setNewPw(e.target.value)}
          autoComplete="new-password"
          disabled={isDemo}
          className={inputClassName}
          style={{ marginBottom: "0.5rem" }}
        />
        <label style={{ fontSize: "0.813rem", fontWeight: 500, display: "block", marginBottom: "0.25rem" }}>
          Confirm new password
        </label>
        <input
          type="password"
          value={confirmPw}
          onChange={(e) => setConfirmPw(e.target.value)}
          autoComplete="new-password"
          disabled={isDemo}
          className={inputClassName}
          style={{ marginBottom: "0.5rem" }}
        />
        <Button
          variant="primary"
          onClick={handleChangePassword}
          disabled={isDemo || !currentPw || !newPw || !confirmPw}
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

      <ApiTokensSection />

      <ProfileDangerZone isDemo={isDemo} username={profile.username} />
    </div>
  );
}

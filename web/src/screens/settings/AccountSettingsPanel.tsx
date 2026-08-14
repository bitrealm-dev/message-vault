import { useState } from "react";
import { apiClient } from "../../lib/api";
import { useAuth } from "../../lib/auth";
import { useAccountProfile } from "../../lib/useAccountProfile";
import Button from "../../components/Button";
import { ProfileDangerZone } from "./ProfileDangerZone";
import { ApiTokensSection } from "./ApiTokensSection";
import { inputClassName, sectionTitleClass } from "./profileStyles";

/** Account settings: username, password, API tokens, danger zone. */
export function AccountSettingsPanel() {
  const { updateToken } = useAuth();
  const { profile, loading, error: loadError } = useAccountProfile();

  const [currentPw, setCurrentPw] = useState("");
  const [newPw, setNewPw] = useState("");
  const [confirmPw, setConfirmPw] = useState("");
  const [pwMsg, setPwMsg] = useState("");
  const [pwOk, setPwOk] = useState(false);

  if (loadError) {
    return (
      <div className="text-danger">
        Could not load account: {loadError}
      </div>
    );
  }

  if (loading || !profile) {
    return <div className="text-muted">Loading…</div>;
  }

  const isDemo = profile.is_demo === true;
  const isGuest = profile.is_guest === true;

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
      const res = await apiClient.post<{ ok: boolean; token: string }>(
        "/v1/auth/change-password",
        {
          current_password: currentPw,
          new_password: newPw,
        },
      );
      if (res.token) updateToken(res.token);
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
      <div className="mb-6 max-w-[360px]">
        <input
          type="text"
          value={profile.username}
          readOnly
          className={`${inputClassName} !text-muted`}
        />
      </div>
      {isGuest ? (
        <p className="mb-6 text-[0.875rem] text-muted">
          This is a temporary sample account. It is removed after 24 hours or when you sign out.
        </p>
      ) : (
        <>
          <h3 className={sectionTitleClass}>Change Password</h3>
          <div className="mb-6 max-w-[360px]">
            <label className="mb-1 block text-[0.813rem] font-medium">
              Current password
            </label>
            <input
              type="password"
              value={currentPw}
              onChange={(e) => setCurrentPw(e.target.value)}
              autoComplete="current-password"
              disabled={isDemo}
              className={`${inputClassName} mb-2`}
            />
            <label className="mb-1 block text-[0.813rem] font-medium">
              New password
            </label>
            <input
              type="password"
              value={newPw}
              onChange={(e) => setNewPw(e.target.value)}
              autoComplete="new-password"
              disabled={isDemo}
              className={`${inputClassName} mb-2`}
            />
            <label className="mb-1 block text-[0.813rem] font-medium">
              Confirm new password
            </label>
            <input
              type="password"
              value={confirmPw}
              onChange={(e) => setConfirmPw(e.target.value)}
              autoComplete="new-password"
              disabled={isDemo}
              className={`${inputClassName} mb-2`}
            />
            <Button
              variant="primary"
              onClick={handleChangePassword}
              disabled={isDemo || !currentPw || !newPw || !confirmPw}
              className="!px-3 !py-1.5"
            >
              Change password
            </Button>
            {pwMsg && (
              <div
                className="mt-1.5 text-[0.813rem]"
                style={{ color: pwOk ? "var(--ok)" : "var(--danger)" }}
              >
                {pwMsg}
              </div>
            )}
          </div>

          <ApiTokensSection />
        </>
      )}

      <ProfileDangerZone isDemo={isDemo} username={profile.username} />
    </div>
  );
}

import { useState } from "react";
import Button from "../../components/Button";
import { useAuth } from "../../lib/auth";
import { useAccountProfile } from "../../lib/useAccountProfile";
import { changePassword } from "../../lib/vaultApi";
import { AddressBookSection } from "./AddressBookSection";
import { ApiTokensSection } from "./ApiTokensSection";
import { ProfileDangerZone } from "./ProfileDangerZone";
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
    return <div className="text-danger">Could not load account: {loadError}</div>;
  }

  if (loading || !profile) {
    return <div className="text-muted">Loading…</div>;
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
      const res = await changePassword({
        current_password: currentPw,
        new_password: newPw,
      });
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
      <h3 className={sectionTitleClass}>Change Password</h3>
      <div className="mb-6 max-w-[360px]">
        <label className="mb-2 block">
          <span className="mb-1 block text-[0.813rem] font-medium">Current password</span>
          <input
            type="password"
            value={currentPw}
            onChange={(e) => setCurrentPw(e.target.value)}
            autoComplete="current-password"
            disabled={isDemo}
            className={inputClassName}
          />
        </label>
        <label className="mb-2 block">
          <span className="mb-1 block text-[0.813rem] font-medium">New password</span>
          <input
            type="password"
            value={newPw}
            onChange={(e) => setNewPw(e.target.value)}
            autoComplete="new-password"
            disabled={isDemo}
            className={inputClassName}
          />
        </label>
        <label className="mb-2 block">
          <span className="mb-1 block text-[0.813rem] font-medium">Confirm new password</span>
          <input
            type="password"
            value={confirmPw}
            onChange={(e) => setConfirmPw(e.target.value)}
            autoComplete="new-password"
            disabled={isDemo}
            className={inputClassName}
          />
        </label>
        <Button
          variant="primary"
          onClick={handleChangePassword}
          disabled={isDemo || !currentPw || !newPw || !confirmPw}
          size="sm"
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

      <AddressBookSection />

      <ApiTokensSection
        accountCanImport={profile.can_import ?? true}
        accountCanExport={profile.can_export ?? true}
        accountCanDelete={profile.can_delete ?? false}
      />

      <ProfileDangerZone isDemo={isDemo} username={profile.username} />
    </div>
  );
}

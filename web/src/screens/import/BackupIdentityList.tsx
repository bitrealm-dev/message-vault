import Button from "../../components/Button";
import { type IdentityService, identityOnProfile, identityService } from "../../lib/backupIdentity";

/**
 * The addresses a backup's device sent from, each marked as on the
 * account's profile or not, with an inline add for the ones that are not.
 * Renders on the identity stop and as a section on Gate 1.
 */
export default function BackupIdentityList({
  identities,
  profile,
  onAdd,
  busy,
}: {
  identities: string[];
  /** Null while the profile is loading or its fetch failed — marks and
   * add buttons need it, so both wait on it rather than guessing. */
  profile: { phones: string[]; emails: string[] } | null;
  onAdd: (value: string, service: IdentityService) => Promise<void>;
  busy?: boolean;
}) {
  if (identities.length === 0) {
    return (
      <p className="m-0 text-[0.813rem] text-muted">
        This backup doesn't record which account it came from.
      </p>
    );
  }

  return (
    <ul className="m-0 flex list-none flex-col gap-2 p-0">
      {identities.map((identity) => {
        const matched = profile != null && identityOnProfile(identity, profile);
        return (
          <li
            key={identity}
            className="flex items-center justify-between gap-3 rounded-lg border border-border px-3 py-2"
          >
            <span className="text-[0.875rem] text-text">{identity}</span>
            {matched ? (
              <span className="text-[0.813rem] text-muted">On your profile</span>
            ) : (
              <span className="flex items-center gap-2">
                <span className="text-[0.813rem] text-muted">Not on your profile</span>
                {profile != null && (
                  <Button
                    variant="ghost"
                    onClick={() => void onAdd(identity, identityService(identity))}
                    disabled={busy}
                  >
                    Add to profile
                  </Button>
                )}
              </span>
            )}
          </li>
        );
      })}
    </ul>
  );
}

/** Account profile returned by GET /v1/account/profile. */
export interface AccountProfile {
  account_id: string;
  username: string;
  preferred_name: string | null;
  phones: string[];
  emails: string[];
  is_demo?: boolean;
  /** May manage users. */
  is_admin?: boolean;
  /** May call the import endpoints. */
  can_import?: boolean;
  /** May call the export endpoints. */
  can_export?: boolean;
  /** May destroy message data. */
  can_delete?: boolean;
}

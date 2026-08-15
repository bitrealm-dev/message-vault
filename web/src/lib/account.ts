/** Account profile returned by GET /v1/account/profile. */
export interface AccountProfile {
  account_id: string;
  username: string;
  preferred_name: string | null;
  phones: string[];
  emails: string[];
  is_demo?: boolean;
  is_guest?: boolean;
  read_only?: boolean;
}

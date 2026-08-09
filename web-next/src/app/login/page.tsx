import { LoginScreen } from "@/components/LoginScreen";
import { getAuthMode, getHankoApiUrl } from "@/lib/authMode";

// Auth mode / Hanko URL come from runtime env (VAULT_AUTH, HANKO_API_URL).
// Without this, Next prerenders /login at image build as local auth.
export const dynamic = "force-dynamic";

export default function LoginPage() {
  return (
    <LoginScreen authMode={getAuthMode()} hankoApiUrl={getHankoApiUrl()} />
  );
}

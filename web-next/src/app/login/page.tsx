import { LoginScreen } from "@/components/LoginScreen";

export const dynamic = "force-dynamic";

/** The vault has one login route (`POST /v1/auth/login`), so auth is local. */
export default function LoginPage() {
  return <LoginScreen authMode="local" hankoApiUrl="" />;
}

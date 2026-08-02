import { LoginScreen } from "@/components/LoginScreen";
import { getAuthMode, getHankoApiUrl } from "@/lib/authMode";

export default function LoginPage() {
  return (
    <LoginScreen authMode={getAuthMode()} hankoApiUrl={getHankoApiUrl()} />
  );
}

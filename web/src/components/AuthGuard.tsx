import { Navigate, Outlet } from "react-router-dom";
import { useAuth } from "@/lib/auth";

/**
 * Layout route: renders child routes via <Outlet /> if the user
 * is authenticated and has completed onboarding. Otherwise redirects.
 */
export function AuthGuard() {
  const { isAuthenticated, needsOnboarding } = useAuth();

  if (!isAuthenticated) {
    return <Navigate to="/login" replace />;
  }

  if (needsOnboarding) {
    return <Navigate to="/onboarding" replace />;
  }

  return <Outlet />;
}

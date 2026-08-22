import type { ReactNode } from "react";
import { Navigate, Route, Routes } from "react-router-dom";
import AppLayout from "./components/AppLayout";
import { AuthGuard } from "./components/AuthGuard";
import MessageRoute from "./components/MessageRoute";
import { AuthProvider, useAuth } from "./lib/auth";
import { canUseImportExportWithProfile } from "./lib/desktopFeatures";
import { ThemeProvider } from "./lib/ThemeProvider";
import { isTauri } from "./lib/tauri-check";
import { useAccountProfile } from "./lib/useAccountProfile";
import ExportScreen from "./screens/ExportScreen";
import ImportScreen from "./screens/ImportScreen";
import LoginScreen from "./screens/LoginScreen";
import OnboardingScreen from "./screens/OnboardingScreen";
import RegisterScreen from "./screens/RegisterScreen";
import SettingsScreen from "./screens/SettingsScreen";
import TrashScreen from "./screens/TrashScreen";

/** Import and export stay on the desktop app and are closed to guest sessions. */
function ImportExportRoute({ children }: { children: ReactNode }) {
  const { profile, loading } = useAccountProfile();
  if (!isTauri()) {
    return <Navigate to="/" replace />;
  }
  if (loading) {
    return null;
  }
  if (profile == null || !canUseImportExportWithProfile(true, profile)) {
    return <Navigate to="/" replace />;
  }
  return children;
}

function AppRoutes() {
  const { isAuthenticated, needsOnboarding } = useAuth();

  // Where a signed-in visitor to login/register should go next.
  const signedInDestination = <Navigate to={needsOnboarding ? "/onboarding" : "/"} replace />;

  return (
    <Routes>
      {/* Public routes — redirect to / if already authenticated */}
      <Route path="/login" element={isAuthenticated ? signedInDestination : <LoginScreen />} />
      <Route
        path="/register"
        element={isAuthenticated ? signedInDestination : <RegisterScreen />}
      />
      <Route
        path="/onboarding"
        element={
          isAuthenticated && needsOnboarding ? <OnboardingScreen /> : <Navigate to="/" replace />
        }
      />

      {/* Protected routes — AuthGuard redirects to /login or /onboarding */}
      <Route element={<AuthGuard />}>
        <Route element={<AppLayout />}>
          <Route index element={<></>} />
          <Route path="contacts" element={<></>} />
          <Route path="group/:slug" element={<></>} />
          <Route path="no-group" element={<></>} />
          <Route path="tag/:slug" element={<></>} />
          <Route path="no-tag" element={<></>} />
          <Route path="trash" element={<TrashScreen />} />
          <Route
            path="import"
            element={
              <ImportExportRoute>
                <ImportScreen />
              </ImportExportRoute>
            }
          />
          <Route
            path="export"
            element={
              <ImportExportRoute>
                <ExportScreen />
              </ImportExportRoute>
            }
          />
          <Route path="settings" element={<SettingsScreen />} />
          <Route path="messages/:conversationId" element={<MessageRoute />} />
        </Route>
      </Route>

      {/* Catch-all */}
      <Route path="*" element={<Navigate to="/" replace />} />
    </Routes>
  );
}

export default function App() {
  return (
    <ThemeProvider>
      <AuthProvider>
        <AppRoutes />
      </AuthProvider>
    </ThemeProvider>
  );
}

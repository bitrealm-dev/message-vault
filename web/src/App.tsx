import type { ReactNode } from "react";
import { Routes, Route, Navigate } from "react-router-dom";
import { AuthProvider, useAuth } from "./lib/auth";
import { canUseImportExportWithProfile } from "./lib/desktopFeatures";
import { isTauri } from "./lib/tauri-check";
import { ThemeProvider } from "./lib/ThemeProvider";
import { useAccountProfile } from "./lib/useAccountProfile";
import { AuthGuard } from "./components/AuthGuard";
import AppLayout from "./components/AppLayout";
import LoginScreen from "./screens/LoginScreen";
import RegisterScreen from "./screens/RegisterScreen";
import OnboardingScreen from "./screens/OnboardingScreen";
import TrashScreen from "./screens/TrashScreen";
import MessageRoute from "./components/MessageRoute";
import ImportScreen from "./screens/ImportScreen";
import ExportScreen from "./screens/ExportScreen";
import SettingsScreen from "./screens/SettingsScreen";

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
  const signedInDestination = (
    <Navigate to={needsOnboarding ? "/onboarding" : "/"} replace />
  );

  return (
    <Routes>
      {/* Public routes — redirect to / if already authenticated */}
      <Route
        path="/login"
        element={isAuthenticated ? signedInDestination : <LoginScreen />}
      />
      <Route
        path="/register"
        element={isAuthenticated ? signedInDestination : <RegisterScreen />}
      />
      <Route
        path="/onboarding"
        element={
          isAuthenticated && needsOnboarding ? (
            <OnboardingScreen />
          ) : (
            <Navigate to="/" replace />
          )
        }
      />

      {/* Protected routes — AuthGuard redirects to /login or /onboarding */}
      <Route element={<AuthGuard />}>
        <Route element={<AppLayout />}>
          <Route index element={<></>} />
          <Route path="contacts" element={<></>} />
          <Route path="group/:slug" element={<></>} />
          <Route path="no-group" element={<></>} />
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

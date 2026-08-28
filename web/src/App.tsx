import { lazy, type ReactNode, Suspense } from "react";
import { Navigate, Route, Routes } from "react-router-dom";
import AppLayout from "./components/AppLayout";
import { AuthGuard } from "./components/AuthGuard";
import MessageRoute from "./components/MessageRoute";
import { AuthProvider, useAuth } from "./lib/auth";
import { canUseImportExportWithProfile } from "./lib/desktopFeatures";
import { ThemeProvider } from "./lib/ThemeProvider";
import { isTauri } from "./lib/tauri-check";
import { useAccountProfile } from "./lib/useAccountProfile";
import LoginScreen from "./screens/LoginScreen";
import OnboardingScreen from "./screens/OnboardingScreen";
import RegisterScreen from "./screens/RegisterScreen";

/**
 * Import and export only ever run in the desktop app, so their code — the
 * importer forms, the job runner and the Tauri bridge behind them — is split out
 * and never downloaded by a browser visiting the website build.
 */
const ImportScreen = lazy(() => import("./screens/ImportScreen"));
const ExportScreen = lazy(() => import("./screens/ExportScreen"));

/** Settings and trash are their own routes and are not on the first paint path. */
const SettingsScreen = lazy(() => import("./screens/SettingsScreen"));
const TrashScreen = lazy(() => import("./screens/TrashScreen"));

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
  // The chunk only starts loading once the route is allowed, so the redirect
  // paths above never pay for it.
  return <Suspense fallback={null}>{children}</Suspense>;
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
          <Route index element={null} />
          <Route path="contacts" element={null} />
          <Route path="group/:slug" element={null} />
          <Route path="no-group" element={null} />
          <Route path="tag/:slug" element={null} />
          <Route path="no-tag" element={null} />
          <Route
            path="trash"
            element={
              <Suspense fallback={null}>
                <TrashScreen />
              </Suspense>
            }
          />
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
          <Route
            path="settings"
            element={
              <Suspense fallback={null}>
                <SettingsScreen />
              </Suspense>
            }
          />
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

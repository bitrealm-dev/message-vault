import { Routes, Route, Navigate } from "react-router-dom";
import { AuthProvider, useAuth } from "./lib/auth";
import { ThemeProvider } from "./lib/ThemeProvider";
import { AuthGuard } from "./components/AuthGuard";
import AppLayout from "./components/AppLayout";
import LoginScreen from "./screens/LoginScreen";
import RegisterScreen from "./screens/RegisterScreen";
import OnboardingScreen from "./screens/OnboardingScreen";
import TrashScreen from "./screens/TrashScreen";
import ImportScreen from "./screens/ImportScreen";
import ExportScreen from "./screens/ExportScreen";
import SettingsScreen from "./screens/SettingsScreen";

function AppRoutes() {
  const { isAuthenticated, needsOnboarding } = useAuth();

  return (
    <Routes>
      {/* Public routes — redirect to / if already authenticated */}
      <Route
        path="/login"
        element={
          isAuthenticated ? (
            needsOnboarding ? <Navigate to="/onboarding" replace /> : <Navigate to="/" replace />
          ) : (
            <LoginScreen />
          )
        }
      />
      <Route
        path="/register"
        element={
          isAuthenticated ? (
            needsOnboarding ? <Navigate to="/onboarding" replace /> : <Navigate to="/" replace />
          ) : (
            <RegisterScreen />
          )
        }
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
          <Route path="trash" element={<TrashScreen />} />
          <Route path="import" element={<ImportScreen />} />
          <Route path="export" element={<ExportScreen />} />
          <Route path="settings" element={<SettingsScreen />} />
          {/* /messages/:id route added in Task 9 when MessageRoute.tsx is created */}
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

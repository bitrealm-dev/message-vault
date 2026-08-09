import { useState } from "react";
import { AuthProvider, useAuth } from "./lib/auth";
import { ThemeProvider } from "./lib/ThemeProvider";
import AppLayout from "./components/AppLayout";
import LoginScreen from "./screens/LoginScreen";
import RegisterScreen from "./screens/RegisterScreen";
import OnboardingScreen from "./screens/OnboardingScreen";

function AppContent() {
  const { isAuthenticated, needsOnboarding, serverUrl } = useAuth();
  const [view, setView] = useState<"login" | "register">("login");

  if (isAuthenticated && needsOnboarding) {
    return <OnboardingScreen />;
  }

  if (isAuthenticated) {
    return <AppLayout />;
  }

  if (view === "register") {
    return (
      <RegisterScreen
        serverUrl={serverUrl}
        onBack={() => setView("login")}
      />
    );
  }

  return <LoginScreen onRegister={() => setView("register")} />;
}

export default function App() {
  return (
    <ThemeProvider>
      <AuthProvider>
        <AppContent />
      </AuthProvider>
    </ThemeProvider>
  );
}

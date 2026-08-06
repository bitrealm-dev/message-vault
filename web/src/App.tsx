import { useState } from "react";
import { AuthProvider, useAuth } from "./lib/auth";
import AppLayout from "./components/AppLayout";
import LoginScreen from "./screens/LoginScreen";
import RegisterScreen from "./screens/RegisterScreen";

function AppContent() {
  const { isAuthenticated, serverUrl } = useAuth();
  const [view, setView] = useState<"login" | "register">("login");

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
    <AuthProvider>
      <AppContent />
    </AuthProvider>
  );
}

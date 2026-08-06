import { AuthProvider, useAuth } from "./lib/auth";
import AppLayout from "./components/AppLayout";
import LoginScreen from "./screens/LoginScreen";

function AppContent() {
  const { isAuthenticated } = useAuth();

  if (!isAuthenticated) {
    return <LoginScreen />;
  }

  return (
    <AppLayout>
      <div style={{
        display: "flex", alignItems: "center", justifyContent: "center",
        height: "100%", color: "#9ca3af", fontSize: "0.875rem",
      }}>
        Select a conversation to view messages
      </div>
    </AppLayout>
  );
}

export default function App() {
  return (
    <AuthProvider>
      <AppContent />
    </AuthProvider>
  );
}

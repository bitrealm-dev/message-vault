import { AuthProvider, useAuth } from "./lib/auth";
import AppLayout from "./components/AppLayout";
import LoginScreen from "./screens/LoginScreen";

function AppContent() {
  const { isAuthenticated } = useAuth();
  return isAuthenticated ? <AppLayout /> : <LoginScreen />;
}

export default function App() {
  return (
    <AuthProvider>
      <AppContent />
    </AuthProvider>
  );
}

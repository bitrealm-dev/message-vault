import { useState, useEffect } from "react";
import { useAuth } from "../lib/auth";
import { apiClient } from "../lib/api";

interface Profile {
  name: string;
  handles: { handle: string; service: string }[];
  storage: { messages: number; attachments: number; conversations: number };
}

export default function ProfileScreen() {
  const { logout } = useAuth();
  const [profile, setProfile] = useState<Profile | null>(null);
  const [name, setName] = useState("");
  const [saved, setSaved] = useState(false);

  useEffect(() => {
    apiClient.get<Profile>("/v1/account/profile").then((p) => { setProfile(p); setName(p.name); }).catch(() => {});
  }, []);

  if (!profile) return <div style={{ padding: "1.5rem", color: "#9ca3af" }}>Loading…</div>;

  const handleSaveName = async () => {
    await apiClient.post("/v1/account/profile", { name });
    setSaved(true);
    setTimeout(() => setSaved(false), 2000);
  };

  return (
    <div style={{ padding: "1.5rem", maxWidth: "700px" }}>
      <h2 style={{ margin: "0 0 1.5rem 0" }}>My Profile</h2>

      <h3 style={{ fontSize: "0.875rem", color: "#6b7280", marginBottom: "0.5rem" }}>Display Name</h3>
      <div style={{ display: "flex", gap: "0.5rem", marginBottom: "1.5rem" }}>
        <input type="text" value={name} onChange={(e) => setName(e.target.value)}
          style={{ flex: 1, padding: "0.25rem 0.5rem", fontSize: "0.875rem" }} />
        <button onClick={handleSaveName} style={{ padding: "0.25rem 1rem", fontWeight: 600 }}>{saved ? "Saved" : "Save"}</button>
      </div>

      <h3 style={{ fontSize: "0.875rem", color: "#6b7280", marginBottom: "0.5rem" }}>My Handles</h3>
      {profile.handles.map((h, i) => (
        <div key={i} style={{ display: "flex", gap: "1rem", padding: "0.375rem 0", borderBottom: "1px solid #f3f4f6", fontSize: "0.875rem" }}>
          <span style={{ flex: 1 }}>{h.handle}</span>
          <span style={{ color: "#6b7280" }}>{h.service}</span>
        </div>
      ))}

      <h3 style={{ fontSize: "0.875rem", color: "#6b7280", margin: "1.5rem 0 0.5rem" }}>Storage</h3>
      <div style={{ fontSize: "0.875rem", color: "#374151" }}>
        <div>{profile.storage.messages.toLocaleString()} messages</div>
        <div>{profile.storage.attachments.toLocaleString()} attachments</div>
        <div>{profile.storage.conversations.toLocaleString()} conversations</div>
      </div>

      <div style={{ marginTop: "2rem", paddingTop: "1rem", borderTop: "1px solid #e5e7eb" }}>
        <button onClick={logout}
          style={{ color: "#dc2626", border: "1px solid #fecaca", background: "#fef2f2", padding: "0.5rem 1rem", borderRadius: "4px", cursor: "pointer", fontSize: "0.875rem" }}>
          Sign out
        </button>
      </div>
    </div>
  );
}

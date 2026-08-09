import { open } from "@tauri-apps/plugin-dialog";
import Button from "./Button";

interface PathPickerProps {
  value: string;
  onChange: (path: string) => void;
  directory?: boolean;
  placeholder?: string;
}

export default function PathPicker({ value, onChange, directory, placeholder }: PathPickerProps) {
  const browse = async () => {
    const result = directory
      ? await open({ directory: true, multiple: false })
      : await open({ multiple: false });
    if (result && typeof result === "string") {
      onChange(result);
    }
  };

  return (
    <div style={{ display: "flex", gap: "0.5rem", flex: 1 }}>
      <input
        type="text"
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder={placeholder}
        style={{
          flex: 1,
          padding: "0.25rem 0.5rem",
          fontSize: "0.875rem",
          background: "var(--bg)",
          color: "var(--text)",
          border: "1px solid var(--border)",
          borderRadius: "4px",
        }}
      />
      <Button onClick={browse} style={{ padding: "0.25rem 0.75rem", fontSize: "0.813rem" }}>
        Browse
      </Button>
    </div>
  );
}

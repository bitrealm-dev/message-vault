import { open } from "@tauri-apps/plugin-dialog";
import Button from "./Button";
import TextField from "./TextField";

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
    <div className="flex flex-1 gap-2">
      <TextField value={value} onChange={onChange} placeholder={placeholder} className="flex-1" />
      <Button onClick={browse} className="!px-3 !py-1 !text-[0.813rem]">
        Browse
      </Button>
    </div>
  );
}

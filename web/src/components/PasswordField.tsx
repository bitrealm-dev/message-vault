import { TextField, Input, type TextFieldProps } from "react-aria-components";
import Button from "./Button";

export default function PasswordField(
  props: TextFieldProps & { showPassword: boolean; onToggle: () => void },
) {
  const { showPassword, onToggle, ...rest } = props;
  return (
    <TextField {...rest}>
      <div className="flex items-center rounded border border-border bg-elevated focus-within:border-accent">
        <span className="pl-2 text-muted" aria-hidden="true">🔒</span>
        <Input
          type={showPassword ? "text" : "password"}
          className="flex-1 border-none bg-transparent px-2 py-2 text-[0.875rem] text-text outline-none"
        />
        <Button
          variant="ghost"
          onPress={onToggle}
          className="border-none px-2 text-muted"
          style={{ padding: "0.5rem" }}
          aria-label={showPassword ? "Hide password" : "Show password"}
        >
          {showPassword ? "👁" : "👁‍🗨"}
        </Button>
      </div>
    </TextField>
  );
}

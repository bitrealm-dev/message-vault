"use client";

import { MAX_PASSWORD_LENGTH } from "@/lib/passwordPolicy";
import { useState } from "react";
import { EyeIcon, EyeOffIcon } from "./icons";

export function CheckMark() {
  return (
    <span
      role="img"
      aria-label="Valid"
      className="ml-1 inline-block text-[13px] leading-none"
    >
      ✅
    </span>
  );
}

export function PasswordField({
  label,
  value,
  onChange,
  disabled,
  autoComplete,
  showCheck,
}: {
  label: string;
  value: string;
  onChange: (value: string) => void;
  disabled?: boolean;
  autoComplete?: string;
  showCheck?: boolean;
}) {
  const [visible, setVisible] = useState(false);
  return (
    <label className="block">
      <span className="inline-flex items-center text-[13px] text-text">
        {label}
        {showCheck ? <CheckMark /> : null}
      </span>
      <div className="relative mt-1">
        <input
          type={visible ? "text" : "password"}
          value={value}
          onChange={(event) => onChange(event.target.value)}
          disabled={disabled}
          autoComplete={autoComplete}
          maxLength={MAX_PASSWORD_LENGTH - 1}
          className="w-full rounded-md border border-border bg-bg py-2 pl-3 pr-10 text-[14px] text-text outline-none focus:border-accent disabled:opacity-50"
        />
        <button
          type="button"
          tabIndex={-1}
          disabled={disabled}
          aria-label={visible ? "Hide password" : "Show password"}
          onClick={() => setVisible((current) => !current)}
          className="absolute inset-y-0 right-0 flex items-center px-2.5 text-muted transition-colors hover:text-text disabled:opacity-50"
        >
          {visible ? (
            <EyeOffIcon className="size-4" />
          ) : (
            <EyeIcon className="size-4" />
          )}
        </button>
      </div>
    </label>
  );
}

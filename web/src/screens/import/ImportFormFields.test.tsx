/** @vitest-environment jsdom */

import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import ImportFormFields, { type ImportFormFieldsProps } from "./ImportFormFields";

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn(),
}));

afterEach(() => {
  cleanup();
});

const presentFile = { exists: true, isFile: true, isDirectory: false };
const presentDir = { exists: true, isFile: false, isDirectory: true };

function renderForm(override: Partial<ImportFormFieldsProps> = {}) {
  const props: ImportFormFieldsProps = {
    source: "imessage-ios",
    onSourceChange: vi.fn(),
    backupPath: "/backups/iphone",
    onBackupPathChange: vi.fn(),
    backupPassword: "",
    onBackupPasswordChange: vi.fn(),
    showBackupPassword: false,
    onToggleBackupPassword: vi.fn(),
    attachmentRoot: "",
    onAttachmentRootChange: vi.fn(),
    appleContacts: "",
    onAppleContactsChange: vi.fn(),
    pathStats: {
      backup: presentDir,
      attachmentRoot: null,
      appleContacts: null,
      backupEncrypted: false,
    },
    attachmentMedia: "copy",
    onAttachmentMediaChange: vi.fn(),
    maxResolution: "720p",
    onMaxResolutionChange: vi.fn(),
    maxFps: "30",
    onMaxFpsChange: vi.fn(),
    minSizeMb: "20",
    onMinSizeMbChange: vi.fn(),
    contactNameMode: "fill_missing",
    onContactNameModeChange: vi.fn(),
    ownerPhones: [],
    onOwnerPhonesChange: vi.fn(),
    profilePhones: [],
    profilePhonesReady: true,
    profilePhonesError: false,
    showMissingAccountPhoneWarning: false,
    formatOpen: true,
    onToggleFormat: vi.fn(),
    processingOpen: false,
    onToggleProcessing: vi.fn(),
    force: false,
    onForceChange: vi.fn(),
    obfuscate: false,
    onObfuscateChange: vi.fn(),
    running: false,
    onImport: vi.fn(),
    ...override,
  };
  return render(<ImportFormFields {...props} />);
}

describe("ImportFormFields iMessage methods", () => {
  it("shows one iMessage source and an extraction method dropdown", () => {
    renderForm();
    expect(screen.getByLabelText("Import source")).toBeTruthy();
    expect(screen.getByLabelText("Extraction method")).toBeTruthy();
    expect(screen.queryByText("iPhone - iOS")).toBeNull();
    expect(screen.queryByText("iMessage - macOS")).toBeNull();
  });

  it("shows password and hides attachment folder on iPhone backup", () => {
    renderForm({ source: "imessage-ios" });
    expect(screen.getByLabelText("Encryption password")).toBeTruthy();
    expect(screen.queryByLabelText("Attachment folder")).toBeNull();
    expect(screen.queryByLabelText("Apple Contacts file")).toBeNull();
    expect(screen.getByRole("button", { name: "Import" })).not.toBeDisabled();
  });

  it("shows optional attachment folder on Mac Messages", () => {
    renderForm({
      source: "imessage-macos",
      backupPath: "/Users/sam/Library/Messages/chat.db",
      pathStats: {
        backup: presentFile,
        attachmentRoot: null,
        appleContacts: null,
        backupEncrypted: null,
      },
    });
    expect(screen.queryByLabelText("Encryption password")).toBeNull();
    expect(screen.getByLabelText("Attachment folder")).toBeTruthy();
    expect(screen.getByLabelText("Apple Contacts file")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Import" })).not.toBeDisabled();
  });

  it("disables Import on jailbreak until the attachment folder is set", () => {
    renderForm({
      source: "imessage-jailbreak",
      backupPath: "/mnt/iphone/sms.db",
      attachmentRoot: "",
      pathStats: {
        backup: presentFile,
        attachmentRoot: null,
        appleContacts: null,
        backupEncrypted: null,
      },
    });
    expect(screen.getByRole("button", { name: "Import" })).toBeDisabled();
  });

  it("enables jailbreak Import when sms.db and attachment folder exist", () => {
    renderForm({
      source: "imessage-jailbreak",
      backupPath: "/mnt/iphone/sms.db",
      attachmentRoot: "/mnt/iphone/Library/SMS",
      pathStats: {
        backup: presentFile,
        attachmentRoot: presentDir,
        appleContacts: null,
        backupEncrypted: null,
      },
    });
    expect(screen.getByRole("button", { name: "Import" })).not.toBeDisabled();
  });

  it("shows an attachment-folder kind error when the path is a file", () => {
    renderForm({
      source: "imessage-macos",
      backupPath: "/tmp/chat.db",
      attachmentRoot: "/tmp/chat.db",
      pathStats: {
        backup: presentFile,
        attachmentRoot: presentFile,
        appleContacts: null,
        backupEncrypted: null,
      },
    });
    expect(
      screen.getByText("Pick the folder that contains Attachments and StickerCache."),
    ).toBeTruthy();
    expect(screen.getByRole("button", { name: "Import" })).toBeDisabled();
  });

  it("shows an Apple Contacts kind error when the path is a directory", () => {
    renderForm({
      source: "imessage-macos",
      backupPath: "/tmp/chat.db",
      appleContacts: "/tmp/AddressBook",
      pathStats: {
        backup: presentFile,
        attachmentRoot: null,
        appleContacts: presentDir,
        backupEncrypted: null,
      },
    });
    expect(screen.getByText("Pick AddressBook-v22.abcddb or AddressBook.sqlitedb.")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Import" })).toBeDisabled();
  });
});

import { IMESSAGE_SOURCE_ID } from "./imessageImport";

/** Backup sources offered by Import in the desktop app. */
export const EXPORT_SOURCES: { id: string; label: string }[] = [
  { id: IMESSAGE_SOURCE_ID, label: "iMessage" },
  { id: "whatsapp-android", label: "WhatsApp - Android" },
  { id: "whatsapp-ios", label: "WhatsApp - iOS" },
  { id: "sms-backup-restore", label: "SMS Backup & Restore" },
  { id: "go-sms-pro", label: "GO SMS Pro" },
  { id: "imazing", label: "iMazing" },
  { id: "sms-backup-plus", label: "SMS Backup+" },
  { id: "openextract", label: "OpenExtract" },
];

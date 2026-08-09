/** Display label for a message source id (e.g. imessage → iMessage). */
export function formatSourceLabel(id: string): string {
  const known: Record<string, string> = {
    imessage: "iMessage",
    "go-sms-pro": "GO SMS Pro",
    "sms-backup-plus": "SMS Backup Plus",
    "sms-backup-restore": "SMS Backup Restore",
  };
  if (known[id]) return known[id];
  return id
    .split("-")
    .filter(Boolean)
    .map((w) => w.charAt(0).toUpperCase() + w.slice(1))
    .join(" ");
}

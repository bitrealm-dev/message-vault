import { IMESSAGE_SOURCE_ID, isImessageMethod } from "./imessageImport";
import { WHATSAPP_SOURCE_ID, isWhatsappMethod } from "./whatsappImport";

/** Vault session / messages.source slug for a desktop Import method id. */
export function vaultSourceForMethod(source: string): string {
  if (isImessageMethod(source)) {
    return IMESSAGE_SOURCE_ID;
  }
  if (isWhatsappMethod(source)) {
    return WHATSAPP_SOURCE_ID;
  }
  return source;
}

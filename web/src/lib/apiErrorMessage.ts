/**
 * Pull the human-readable part out of an API error.
 *
 * `apiClient` rejects with `Error("<status>: <body>")`, and the body is usually
 * `{"error": "..."}`. Shows that string when it is there, the raw body when it
 * is not JSON, and `fallback` when the rejection is not an Error at all.
 */
export function apiErrorMessage(err: unknown, fallback: string): string {
  if (!(err instanceof Error)) return fallback;
  const match = err.message.match(/^\d+:\s*([\s\S]*)$/);
  if (!match) return err.message || fallback;
  try {
    const parsed: unknown = JSON.parse(match[1]);
    if (
      typeof parsed === "object" &&
      parsed !== null &&
      "error" in parsed &&
      typeof (parsed as { error: unknown }).error === "string"
    ) {
      return (parsed as { error: string }).error;
    }
  } catch {
    // Body was not JSON; show the raw text.
  }
  return match[1] || fallback;
}

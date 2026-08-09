/** Build the vault asset GET path (query required by the server). */
export function buildAssetPath(sha256: string, source: string): string {
  const sha = sha256.trim();
  const src = source.trim();
  if (!sha) throw new Error("sha256 is required");
  if (!src) throw new Error("source is required");
  return `/v1/assets/${encodeURIComponent(sha)}?source=${encodeURIComponent(src)}`;
}

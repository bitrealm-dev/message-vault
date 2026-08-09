/** URL slug for a contact label. Preserves case so names that only differ by
 *  casing (e.g. Regroup / reGroup / regroup) map to distinct paths. */
export function labelSlug(name: string): string {
  return name
    .trim()
    .replace(/[^a-zA-Z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
}

/** Build label + date baked in at `next build` via NEXT_PUBLIC_* env. */
export type BuildInfo = {
  id: string;
  date: string | null;
};

export function getBuildInfo(): BuildInfo {
  const id = process.env.NEXT_PUBLIC_BUILD_ID?.trim() || "dev";
  const date = process.env.NEXT_PUBLIC_BUILD_DATE?.trim() || null;
  return { id, date };
}

export function formatBuildInfo(info: BuildInfo = getBuildInfo()): string {
  return info.date ? `${info.id} · ${info.date}` : info.id;
}

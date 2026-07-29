import fs from "node:fs";
import path from "node:path";

import { unauthorizedResponse, withAccountHandler } from "@/lib/accountContext";
import { assertVaultWritable, mutationErrorStatus } from "@/lib/owner";
import { configTomlPath, repoRoot } from "@/lib/paths";
import { NextResponse } from "next/server";
import { parse } from "smol-toml";

export const dynamic = "force-dynamic";

const CLI_HINT =
  "Demo reset is CLI-only. From the repo root run: cargo run --release -- reset-demo";

function demoBundleEnabled(): boolean {
  const bundleConfig = path.join(repoRoot(), "demo", "config", "config.toml");
  if (fs.existsSync(bundleConfig)) {
    return true;
  }
  try {
    const cfg = parse(fs.readFileSync(configTomlPath(), "utf8")) as {
      demo?: { enabled?: boolean };
    };
    return cfg.demo?.enabled === true;
  } catch {
    return false;
  }
}

function authError(err: unknown): NextResponse | null {
  if (err instanceof Error && err.message === "Not signed in") {
    return unauthorizedResponse();
  }
  return null;
}

/** Whether the demo bundle is present (UI can show CLI reset instructions). */
export async function GET() {
  return NextResponse.json({
    available: demoBundleEnabled(),
    hint: CLI_HINT,
  });
}

/** Ingest is owned by the Rust server/CLI — web no longer spawns reset-demo. */
export async function POST() {
  try {
    return await withAccountHandler(async () => {
      assertVaultWritable();
      return NextResponse.json(
        { ok: false, error: CLI_HINT },
        { status: 410 },
      );
    });
  } catch (err) {
    const auth = authError(err);
    if (auth) return auth;
    const message = err instanceof Error ? err.message : "read-only";
    return NextResponse.json(
      { ok: false, error: message },
      { status: mutationErrorStatus(message, 500) },
    );
  }
}

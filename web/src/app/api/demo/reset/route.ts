import fs from "node:fs";
import path from "node:path";

import {
  unauthorizedResponse,
  withAccountHandler,
} from "@/lib/accountContext";
import { isDemoAccount } from "@/lib/demoAccount";
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

/** Demo reset menu is only for the signed-in demo account when the bundle exists. */
export async function GET() {
  try {
    return await withAccountHandler(async (accountId) => {
      const available = isDemoAccount(accountId) && demoBundleEnabled();
      return NextResponse.json({
        available,
        hint: available ? CLI_HINT : null,
      });
    });
  } catch (err) {
    const auth = authError(err);
    if (auth) return auth;
    return NextResponse.json({ available: false, hint: null });
  }
}

/** Ingest is owned by the Rust server/CLI — web no longer spawns reset-demo. */
export async function POST() {
  try {
    return await withAccountHandler(async (accountId) => {
      if (!isDemoAccount(accountId)) {
        return NextResponse.json(
          { ok: false, error: "Demo reset is only available for the demo account." },
          { status: 403 },
        );
      }
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

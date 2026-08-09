import { sourceById } from "@/lib/paths";
import {
  unauthorizedResponse,
  withAccountHandler,
} from "@/lib/accountContext";
import fs from "fs";
import path from "path";
import { NextResponse } from "next/server";

export const runtime = "nodejs";

type Params = { params: Promise<{ source: string; path: string[] }> };

const MIME: Record<string, string> = {
  ".jpg": "image/jpeg",
  ".jpeg": "image/jpeg",
  ".png": "image/png",
  ".gif": "image/gif",
  ".webp": "image/webp",
  ".heic": "image/heic",
  ".mp4": "video/mp4",
  ".mov": "video/quicktime",
  ".pdf": "application/pdf",
  ".mp3": "audio/mpeg",
  ".m4a": "audio/mp4",
};

function serveFromRoot(root: string, parts: string[]) {
  const rel = parts.join("/");
  if (rel.includes("..") || path.isAbsolute(rel)) {
    return NextResponse.json({ error: "invalid path" }, { status: 400 });
  }
  const full = path.resolve(root, rel);
  if (!full.startsWith(path.resolve(root))) {
    return NextResponse.json({ error: "invalid path" }, { status: 400 });
  }
  if (!fs.existsSync(full) || !fs.statSync(full).isFile()) {
    return NextResponse.json({ error: "not found" }, { status: 404 });
  }
  const ext = path.extname(full).toLowerCase();
  const mime = MIME[ext] ?? "application/octet-stream";
  const buf = fs.readFileSync(full);
  return new NextResponse(buf, {
    headers: {
      "Content-Type": mime,
      "Cache-Control": "public, max-age=31536000, immutable",
    },
  });
}

function authError(err: unknown): NextResponse | null {
  if (err instanceof Error && err.message === "Not signed in") {
    return unauthorizedResponse();
  }
  return null;
}

export async function GET(_req: Request, { params }: Params) {
  const { source, path: parts } = await params;
  try {
    return await withAccountHandler(async () => {
      const src = sourceById(source);
      if (!src) {
        return NextResponse.json({ error: "unknown source" }, { status: 404 });
      }
      return serveFromRoot(src.assetsDir, parts);
    });
  } catch (err) {
    const auth = authError(err);
    if (auth) return auth;
    const message = err instanceof Error ? err.message : "serve failed";
    return NextResponse.json({ error: message }, { status: 500 });
  }
}

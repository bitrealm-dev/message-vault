import path from "node:path";
import type { NextConfig } from "next";

const nextConfig: NextConfig = {
  // Slimmer Docker release image (Dockerfile.release copies .next/standalone).
  output: "standalone",
  // Keep file tracing inside web/ (a root package-lock.json otherwise pulls in the repo).
  outputFileTracingRoot: path.join(__dirname),
  turbopack: {
    root: path.join(__dirname),
  },
  serverExternalPackages: ["better-sqlite3"],
  allowedDevOrigins: ["192.168.50.100"],
  async redirects() {
    return [
      { source: "/group-chats", destination: "/group-messages", permanent: false },
      { source: "/group-chats-2", destination: "/group-messages", permanent: false },
      { source: "/group/:slug", destination: "/label/:slug", permanent: false },
      { source: "/no-group", destination: "/no-label", permanent: false },
      { source: "/unassigned", destination: "/all", permanent: false },
    ];
  },
};

export default nextConfig;

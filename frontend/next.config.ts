import type { NextConfig } from "next";

const nextConfig: NextConfig = {
  agentRules: false,
  async rewrites() {
    const backend = (process.env.BACKEND_INTERNAL_URL ?? "http://localhost:8000").replace(/\/$/, "");
    // One public URL/tunnel can serve the frontend and the Rust API/MCP.
    return ["/mcp", "/.well-known/:path*", "/oauth/authorize", "/oauth/token", "/oauth/revoke", "/health/live", "/whoami", "/portfolio", "/api/transfers/:path*"].map(source => ({ source, destination: `${backend}${source}` }));
  },
  async headers() {
    return ["/auth/:path*", "/transfers/:path*"].map(source => ({ source, headers: [
      { key: "Referrer-Policy", value: "no-referrer" },
      { key: "X-Frame-Options", value: "DENY" },
    ] }));
  },
  /* config options here */
  experimental: {
    agentFeedback: false,
  },
  cacheComponents: true,
  partialPrefetching: true,
  turbopack: {
    rules: {
      "*.css": {
        loaders: ["@tailwindcss/turbopack"],
        as: "*.css",
      },
    },
  },
};

export default nextConfig;

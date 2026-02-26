import type { NextConfig } from "next";

const nextConfig: NextConfig = {
  // Use standalone mode for a self-contained Node.js server.
  // This works in both dev and production, eliminating the need for
  // generateStaticParams placeholders.
  output: "standalone",

  // Rewrites proxy API calls to the ACS daemon.
  // Unlike static export, standalone mode supports rewrites in all environments.
  async rewrites() {
    return [
      {
        source: "/api/:path*",
        destination: "http://127.0.0.1:8377/api/:path*",
      },
      {
        source: "/health",
        destination: "http://127.0.0.1:8377/health",
      },
    ];
  },
};

export default nextConfig;

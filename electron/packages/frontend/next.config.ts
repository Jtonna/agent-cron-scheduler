import type { NextConfig } from "next";

const nextConfig: NextConfig = {
  output: "export",
  images: {
    unoptimized: true,
  },
  // Rewrites only work in dev mode with static export
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

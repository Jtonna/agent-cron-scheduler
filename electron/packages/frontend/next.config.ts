import type { NextConfig } from "next";

const nextConfig: NextConfig = {
  // Only enable static export for production builds.
  // In dev mode, this must be off so dynamic routes work without being
  // pre-listed in generateStaticParams().
  ...(process.env.NODE_ENV === "production"
    ? { output: "export" }
    : {}),

  // Rewrites proxy API calls to the ACS daemon during local dev.
  // Must be excluded in production because Next.js does not allow rewrites
  // alongside output: "export".
  ...(process.env.NODE_ENV !== "production"
    ? {
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
      }
    : {}),
};

export default nextConfig;

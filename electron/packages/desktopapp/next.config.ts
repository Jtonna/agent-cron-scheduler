import type { NextConfig } from "next";
import path from "path";

const nextConfig: NextConfig = {
  // Pin tracing root to this package so Next doesn't walk up to electron/package-lock.json.
  // desktopapp is intentionally excluded from the electron workspace (see electron/package.json)
  // and installs standalone; without this, Next picks the wrong lockfile and warns.
  outputFileTracingRoot: path.join(__dirname),
};

export default nextConfig;

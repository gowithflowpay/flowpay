import path from "node:path";
import nextEnv from "@next/env";

const { loadEnvConfig } = nextEnv;

const projectRoot = path.resolve(process.cwd(), "../..");
loadEnvConfig(projectRoot);

/** @type {import("next").NextConfig} */
const nextConfig = {
  distDir: process.env.FLOWPAY_NEXT_DIST_DIR || ".next",
  outputFileTracingRoot: process.env.VERCEL ? process.cwd() : projectRoot,
};

export default nextConfig;

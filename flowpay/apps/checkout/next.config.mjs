import path from "node:path";
import nextEnv from "@next/env";

const { loadEnvConfig } = nextEnv;

const projectRoot = path.resolve(process.cwd(), "../..");
loadEnvConfig(projectRoot);

/** @type {import("next").NextConfig} */
const nextConfig = {
  outputFileTracingRoot: projectRoot,
};

export default nextConfig;

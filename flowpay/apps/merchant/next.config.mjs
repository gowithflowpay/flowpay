import path from "node:path";
import nextEnv from "@next/env";

const { loadEnvConfig } = nextEnv;

const projectRoot = path.resolve(process.cwd(), "../..");
loadEnvConfig(projectRoot);

/** @type {import("next").NextConfig} */
const nextConfig = {
  outputFileTracingRoot: process.env.VERCEL ? process.cwd() : projectRoot,
  async rewrites() {
    const checkoutBase=(process.env.FLOWPAY_CHECKOUT_BASE_URL||"http://localhost:3001").replace(/\/$/,"");
    return [
      {source:"/pay/:path*",destination:`${checkoutBase}/pay/:path*`},
      {source:"/claim",destination:`${checkoutBase}/claim`},
      {source:"/claim/:path*",destination:`${checkoutBase}/claim/:path*`},
      {source:"/api/payment/:path*",destination:`${checkoutBase}/api/payment/:path*`},
      {source:"/api/claims/:path*",destination:`${checkoutBase}/api/claims/:path*`},
    ];
  },
};

export default nextConfig;

import "server-only";
import path from "node:path";
import { loadEnvConfig } from "@next/env";

// Route handlers execute in their own runtime and do not inherit variables
// loaded while Next evaluates next.config.mjs. Load the repository environment
// in the server-only API module so hosted pages and browser polling agree.
loadEnvConfig(path.resolve(process.cwd(), "../.."));

export async function api(path:string,init:RequestInit={}){
  const base=(process.env.FLOWPAY_API_URL??"http://127.0.0.1:8080").replace(/\/$/,"");
  const key=process.env.FLOWPAY_CHECKOUT_API_KEY??process.env.FLOWPAY_API_KEY??process.env.FLOWPAY_DEMO_API_KEY;
  if(!key&&!path.startsWith("/v1/public/"))throw new Error("Checkout authentication is unavailable. Please try again.");
  const response=await fetch(base+path,{
    ...init,
    cache:"no-store",
    headers:{
      ...(key?{"x-flowpay-api-key":key}:{}),
      "content-type":"application/json",
      "accept":"application/json",
      ...(init.headers??{}),
    },
  });
  const value=await response.json();
  if(!response.ok)throw new Error(value?.error?.message??"FlowPay request failed");
  return value;
}

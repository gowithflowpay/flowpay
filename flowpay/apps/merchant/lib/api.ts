import "server-only";
import {existsSync,readFileSync} from "node:fs";
import path from "node:path";

function envFileValue(name:string){
  const configuredFile=process.env.FLOWPAY_ENV_FILE;
  const files=[
    configuredFile,
    path.resolve(process.cwd(),".env"),
    path.resolve(process.cwd(),"../..", ".env"),
  ].filter((value):value is string=>Boolean(value));
  for(const file of files){
    if(!existsSync(file))continue;
    const line=readFileSync(file,"utf8").split(/\r?\n/).find(value=>new RegExp(`^\\s*${name}\\s*=`).test(value));
    if(!line)continue;
    return line.slice(line.indexOf("=")+1).trim().replace(/^(["'])(.*)\1$/,"$2");
  }
  return undefined;
}

function systemApiKey(){
  // One server-only system credential. The browser and payment form never see it.
  const key=(process.env.FLOWPAY_API_KEY??envFileValue("FLOWPAY_API_KEY"))?.trim();
  if(!key)throw new Error("FLOWPAY_API_KEY is required on the merchant server");
  return key;
}

export async function api(pathname:string,init:RequestInit={}){
  const base=(process.env.FLOWPAY_API_URL??"http://127.0.0.1:8080").replace(/\/$/,"");
  const h=new Headers(init.headers as HeadersInit|undefined);
  h.set("content-type","application/json");
  h.set("x-flowpay-api-key",systemApiKey());
  const r=await fetch(base+pathname,{...init,cache:"no-store",headers:h});
  const text=await r.text();
  let data:any={};
  try{data=text?JSON.parse(text):{};}catch{data={raw:text};}
  if(!r.ok)throw new Error(data?.error?.message??`FlowPay API ${r.status}`);
  return data;
}

#!/usr/bin/env node
import {McpServer} from "@modelcontextprotocol/sdk/server/mcp.js";
import {StdioServerTransport} from "@modelcontextprotocol/sdk/server/stdio.js";
import {mkdir,readFile,writeFile} from "node:fs/promises";
import {existsSync} from "node:fs";
import path from "node:path";
import {z} from "zod";

const root=path.resolve(process.env.FLOWPAY_PROJECT_ROOT??process.cwd());
const apiBase=(process.env.FLOWPAY_API_URL??"https://api.pixuno.xyz").replace(/\/$/,"");
const checkoutBase=(process.env.FLOWPAY_CHECKOUT_URL??"https://checkout.pixuno.xyz").replace(/\/$/,"");

function inside(relative:string){
  const target=path.resolve(root,relative);
  if(target!==root&&!target.startsWith(root+path.sep))throw new Error("generated path escaped project root");
  return target;
}

async function jsonRequest(route:string,init:RequestInit){
  const response=await fetch(apiBase+route,init);
  const text=await response.text();
  let body:any={};
  try{body=text?JSON.parse(text):{};}catch{}
  if(!response.ok)throw new Error(body?.error?.message??`FlowPay API returned ${response.status}`);
  return body;
}

async function provisionCredential(label:string){
  const existing=process.env.FLOWPAY_API_KEY?.trim();
  if(existing)return existing;
  const token=process.env.FLOWPAY_MERCHANT_TOKEN?.trim();
  if(!token)throw new Error("Configure FLOWPAY_API_KEY or FLOWPAY_MERCHANT_TOKEN in the MCP server environment");
  const result=await jsonRequest("/v1/api-keys",{
    method:"POST",
    headers:{authorization:`Bearer ${token}`,"content-type":"application/json"},
    body:JSON.stringify({name:label}),
  });
  const credential=String(result.api_key??"").trim();
  if(!credential)throw new Error("FlowPay did not return the one-time API credential");
  return credential;
}

async function mergeEnv(relative:string,values:Record<string,string>){
  const target=inside(relative);
  let current="";
  try{current=await readFile(target,"utf8");}catch{}
  const lines=current.split(/\r?\n/).filter(Boolean);
  for(const [key,value] of Object.entries(values)){
    const index=lines.findIndex(line=>line.startsWith(`${key}=`));
    const next=`${key}=${value}`;
    if(index>=0)lines[index]=next;else lines.push(next);
  }
  await writeFile(target,lines.join("\n")+"\n",{encoding:"utf8",mode:0o600});
}

const server=new McpServer({name:"flowpay-integration",version:"0.1.0"});

server.registerTool("flowpay_inspect_project",{
  title:"Inspect site for FlowPay integration",
  description:"Detect the application framework and report the exact crypto checkout integration plan. This never reads or returns secret files.",
  inputSchema:{},
},async()=>{
  const packagePath=inside("package.json");
  if(!existsSync(packagePath))return {content:[{type:"text",text:JSON.stringify({root,framework:"unknown",supported:false})}]};
  const pkg=JSON.parse(await readFile(packagePath,"utf8"));
  const deps={...pkg.dependencies,...pkg.devDependencies};
  const framework=deps.next?"nextjs":deps["@remix-run/react"]?"remix":deps.vite?"vite":"node";
  return {content:[{type:"text",text:JSON.stringify({root,framework,supported:framework==="nextjs",plan:["create server-only FlowPay route","store credential in .env.local","add checkout button","verify generated files"]})}]};
});

server.registerTool("flowpay_install_nextjs",{
  title:"Install FlowPay crypto checkout",
  description:"Creates a complete Next.js App Router crypto checkout integration. It provisions or reuses a credential inside the MCP process and writes it directly to .env.local; the credential is never returned to the model.",
  inputSchema:{
    route:z.string().regex(/^\/[a-zA-Z0-9/_-]*$/).default("/api/flowpay/create-payment"),
    componentPath:z.string().default("components/FlowPayButton.tsx"),
    label:z.string().min(1).max(80).default("AI-generated site integration"),
  },
},async({route,componentPath,label})=>{
  const pkgPath=inside("package.json");
  if(!existsSync(pkgPath))throw new Error("package.json was not found in FLOWPAY_PROJECT_ROOT");
  const pkg=JSON.parse(await readFile(pkgPath,"utf8"));
  if(!({...pkg.dependencies,...pkg.devDependencies}).next)throw new Error("flowpay_install_nextjs requires a Next.js project");
  const credential=await provisionCredential(label);
  await mergeEnv(".env.local",{FLOWPAY_API_URL:apiBase,FLOWPAY_CHECKOUT_URL:checkoutBase,FLOWPAY_API_KEY:credential});
  const routeFile=`app${route}/route.ts`.replace(/\/+/g,"/");
  await mkdir(path.dirname(inside(routeFile)),{recursive:true});
  await writeFile(inside(routeFile),`import {NextResponse} from "next/server";\n\nexport async function POST(request:Request){\n  const input=await request.json();\n  const response=await fetch(\`${apiBase}/v1/payments\`,{method:"POST",headers:{authorization:\`Bearer \${process.env.FLOWPAY_API_KEY}\`,"content-type":"application/json","idempotency-key":crypto.randomUUID()},body:JSON.stringify({amount:String(input.amount),asset:input.asset??"USDC",chain:input.chain??"base",reference:input.reference})});\n  const body=await response.json();\n  return NextResponse.json(body,{status:response.status});\n}\n`,"utf8");
  await mkdir(path.dirname(inside(componentPath)),{recursive:true});
  await writeFile(inside(componentPath),`"use client";\nimport {useState} from "react";\n\nexport function FlowPayButton({amount,reference,asset="USDC",chain="base"}:{amount:string;reference?:string;asset?:"USDC"|"USDT"|"ETH";chain?:string}){\n  const [busy,setBusy]=useState(false);\n  const [error,setError]=useState("");\n  async function pay(){setBusy(true);setError("");try{const response=await fetch("${route}",{method:"POST",headers:{"content-type":"application/json"},body:JSON.stringify({amount,reference,asset,chain})});const body=await response.json();if(!response.ok)throw new Error(body?.error?.message??"Unable to create payment");window.location.assign(body.checkout_url);}catch(value){setError(value instanceof Error?value.message:"Unable to create payment");setBusy(false)}}\n  return <div><button type="button" onClick={pay} disabled={busy}>{busy?"Opening secure checkout…":"Pay with crypto"}</button>{error?<p role="alert">{error}</p>:null}</div>;\n}\n`,"utf8");
  return {content:[{type:"text",text:JSON.stringify({installed:true,framework:"nextjs",files:[routeFile,componentPath,".env.local"],credential:"stored directly; not exposed",next:`Import FlowPayButton from ${componentPath} and render it with the order amount and reference.`})}]};
});

server.registerTool("flowpay_verify_integration",{
  title:"Verify FlowPay integration",
  description:"Checks generated files and validates the configured credential against FlowPay without returning it.",
  inputSchema:{route:z.string().default("app/api/flowpay/create-payment/route.ts"),componentPath:z.string().default("components/FlowPayButton.tsx")},
},async({route,componentPath})=>{
  const env=await readFile(inside(".env.local"),"utf8").catch(()=>"");
  const key=env.match(/^FLOWPAY_API_KEY=(.+)$/m)?.[1]?.trim();
  const files={route:existsSync(inside(route)),component:existsSync(inside(componentPath)),env:Boolean(key)};
  let api=false;
  if(key){try{await jsonRequest("/v1/payments?limit=1",{headers:{authorization:`Bearer ${key}`}});api=true;}catch{}}
  return {content:[{type:"text",text:JSON.stringify({ok:Object.values(files).every(Boolean)&&api,files,api_authenticated:api,secrets_exposed:false})}]};
});

await server.connect(new StdioServerTransport());

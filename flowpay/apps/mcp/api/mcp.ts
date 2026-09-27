import {McpServer} from "@modelcontextprotocol/sdk/server/mcp.js";
import {WebStandardStreamableHTTPServerTransport} from "@modelcontextprotocol/sdk/server/webStandardStreamableHttp.js";
import {z} from "zod";

const apiBase=(process.env.FLOWPAY_API_URL??"https://api.pixuno.xyz").replace(/\/$/,"");
const checkoutBase=(process.env.FLOWPAY_CHECKOUT_URL??"https://checkout.pixuno.xyz").replace(/\/$/,"");

function cors(headers:Headers){
  headers.set("access-control-allow-origin","*");
  headers.set("access-control-allow-methods","GET,POST,DELETE,OPTIONS");
  headers.set("access-control-allow-headers","authorization,content-type,mcp-protocol-version,mcp-session-id,last-event-id");
  headers.set("access-control-expose-headers","mcp-protocol-version,mcp-session-id");
}

function bearer(request:Request){
  const value=request.headers.get("authorization")??"";
  const match=value.match(/^Bearer\s+(.+)$/i);
  return match?.[1]?.trim()??"";
}

function unauthorized(message="A FlowPay API key is required as a Bearer token"){
  const headers=new Headers({"content-type":"application/json","www-authenticate":'Bearer realm="FlowPay MCP"'});
  cors(headers);
  return new Response(JSON.stringify({error:"unauthorized",message}),{status:401,headers});
}

async function flowpay(key:string,route:string,init:RequestInit={}){
  const headers=new Headers(init.headers);
  headers.set("x-flowpay-api-key",key);
  headers.set("accept","application/json");
  const response=await fetch(apiBase+route,{...init,headers,cache:"no-store"});
  const text=await response.text();
  let body:unknown={};
  try{body=text?JSON.parse(text):{};}catch{body={error:{message:text||`FlowPay returned ${response.status}`}};}
  if(!response.ok){
    const detail=body as {error?:{message?:string;code?:string}};
    throw new Error(detail.error?.message??detail.error?.code??`FlowPay returned ${response.status}`);
  }
  return body;
}

function result(value:unknown){
  return {content:[{type:"text" as const,text:JSON.stringify(value)}],structuredContent:value as Record<string,unknown>};
}

function createServer(key:string){
  const server=new McpServer({name:"FlowPay",version:"1.0.0"});

  server.registerTool("flowpay_integration_guide",{
    title:"Get FlowPay integration plan",
    description:"Returns a credential-safe plan for adding FlowPay crypto checkout to a website. The API key stays in server-side environment variables and is never included in generated browser code.",
    inputSchema:{framework:z.enum(["nextjs","node","other"]).default("nextjs")},
    annotations:{readOnlyHint:true,destructiveHint:false,idempotentHint:true,openWorldHint:false},
  },async({framework})=>result({
    framework,
    api_base:apiBase,
    checkout_base:checkoutBase,
    authentication:"Store FLOWPAY_API_KEY in the deployment secret manager. Send it only from a server route using x-flowpay-api-key.",
    steps:["create a server-only payment endpoint","validate amount, asset, chain, and reference","generate a unique Idempotency-Key","call POST /v1/payments","redirect the browser to checkout_url","verify the integration with a test payment"],
    supported_assets:["USDC","USDT","ETH"],
    fiat_supported:false,
    secrets_exposed:false,
  }));

  server.registerTool("flowpay_generate_nextjs_integration",{
    title:"Generate Next.js FlowPay checkout files",
    description:"Generates complete Next.js App Router files. The output contains only an environment-variable reference, never the FlowPay API key.",
    inputSchema:{route:z.string().regex(/^\/[a-zA-Z0-9/_-]*$/).default("/api/flowpay/create-payment"),componentPath:z.string().default("components/FlowPayButton.tsx")},
    annotations:{readOnlyHint:true,destructiveHint:false,idempotentHint:true,openWorldHint:false},
  },async({route,componentPath})=>{
    const routeFile=`app${route}/route.ts`.replace(/\/+/g,"/");
    const routeSource=`import {NextResponse} from "next/server";\n\nexport async function POST(request:Request){\n  const input=await request.json();\n  const response=await fetch("${apiBase}/v1/payments",{method:"POST",headers:{"x-flowpay-api-key":process.env.FLOWPAY_API_KEY??"","content-type":"application/json","idempotency-key":crypto.randomUUID()},body:JSON.stringify({amount:String(input.amount),asset:input.asset??"USDC",chain:input.chain??"base_sepolia",reference:input.reference})});\n  const body=await response.json();\n  return NextResponse.json(body,{status:response.status});\n}\n`;
    const componentSource=`"use client";\nimport {useState} from "react";\n\nexport function FlowPayButton({amount,reference,asset="USDC",chain="base_sepolia"}:{amount:string;reference?:string;asset?:"USDC"|"USDT"|"ETH";chain?:string}){\n  const [busy,setBusy]=useState(false);\n  const [error,setError]=useState("");\n  async function pay(){setBusy(true);setError("");try{const response=await fetch("${route}",{method:"POST",headers:{"content-type":"application/json"},body:JSON.stringify({amount,reference,asset,chain})});const body=await response.json();if(!response.ok)throw new Error(body?.error?.message??"Unable to create payment");window.location.assign(body.checkout_url);}catch(value){setError(value instanceof Error?value.message:"Unable to create payment");setBusy(false)}}\n  return <div><button type="button" onClick={pay} disabled={busy}>{busy?"Opening secure checkout...":"Pay with crypto"}</button>{error?<p role="alert">{error}</p>:null}</div>;\n}\n`;
    return result({files:[{path:routeFile,content:routeSource},{path:componentPath,content:componentSource}],environment:[{name:"FLOWPAY_API_KEY",location:"server-only secret manager",value_exposed:false}],instructions:[`Write the returned files to the project.`,`Add FLOWPAY_API_KEY through the host's encrypted environment settings.`,`Import FlowPayButton from ${componentPath}.`,`Run flowpay_verify_credentials before production use.`],secrets_exposed:false});
  });

  server.registerTool("flowpay_verify_credentials",{
    title:"Verify FlowPay credentials",
    description:"Tests the connector's stored credential against the live FlowPay API without returning the credential.",
    inputSchema:{},
    annotations:{readOnlyHint:true,destructiveHint:false,idempotentHint:true,openWorldHint:true},
  },async()=>{await flowpay(key,"/v1/payments?limit=1");return result({authenticated:true,api:apiBase,mode:"live",secrets_exposed:false});});

  server.registerTool("flowpay_list_payments",{
    title:"List FlowPay payments",
    description:"Lists crypto payments belonging to the authenticated FlowPay merchant.",
    inputSchema:{limit:z.number().int().min(1).max(100).default(20)},
    annotations:{readOnlyHint:true,destructiveHint:false,idempotentHint:true,openWorldHint:true},
  },async({limit})=>result(await flowpay(key,`/v1/payments?limit=${limit}`)));

  server.registerTool("flowpay_get_payment",{
    title:"Get a FlowPay payment",
    description:"Returns one crypto payment belonging to the authenticated merchant.",
    inputSchema:{paymentId:z.string().min(1)},
    annotations:{readOnlyHint:true,destructiveHint:false,idempotentHint:true,openWorldHint:true},
  },async({paymentId})=>result(await flowpay(key,`/v1/payments/${encodeURIComponent(paymentId)}`)));

  server.registerTool("flowpay_create_payment",{
    title:"Create a FlowPay crypto payment",
    description:"Creates a live hosted crypto checkout after the user confirms the amount, asset, network, and reference. Fiat creation is not supported.",
    inputSchema:{amount:z.string().regex(/^\d+(\.\d+)?$/),asset:z.enum(["USDC","USDT","ETH"]),chain:z.enum(["base_sepolia","ethereum_sepolia","arbitrum_sepolia","bsc_testnet"]),reference:z.string().max(160).optional(),expiresInSeconds:z.number().int().min(60).max(2592000).default(1800)},
    annotations:{readOnlyHint:false,destructiveHint:false,idempotentHint:false,openWorldHint:true},
  },async({amount,asset,chain,reference,expiresInSeconds})=>result(await flowpay(key,"/v1/payments",{method:"POST",headers:{"content-type":"application/json","idempotency-key":crypto.randomUUID()},body:JSON.stringify({amount,asset,chain,reference,expires_in_seconds:expiresInSeconds})})));

  return server;
}

async function handler(request:Request){
  if(request.method==="OPTIONS"){
    const headers=new Headers();cors(headers);return new Response(null,{status:204,headers});
  }
  const key=bearer(request);
  if(!key)return unauthorized();
  try{await flowpay(key,"/v1/payments?limit=1");}
  catch{return unauthorized("The FlowPay API key is invalid, revoked, or unavailable");}
  const transport=new WebStandardStreamableHTTPServerTransport({sessionIdGenerator:undefined,enableJsonResponse:true,maxRequestBodySize:1_048_576});
  const server=createServer(key);
  try{
    await server.connect(transport);
    const response=await transport.handleRequest(request,{authInfo:{token:key,clientId:"flowpay-api-key",scopes:["payments:read","payments:create"]}});
    cors(response.headers);
    return response;
  }finally{
    await transport.close().catch(()=>undefined);
    await server.close().catch(()=>undefined);
  }
}

export const GET=handler;
export const POST=handler;
export const DELETE=handler;
export const OPTIONS=handler;

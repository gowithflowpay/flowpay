import {McpServer} from "@modelcontextprotocol/sdk/server/mcp.js";
import {WebStandardStreamableHTTPServerTransport} from "@modelcontextprotocol/sdk/server/webStandardStreamableHttp.js";
import {z} from "zod";

const apiBase=(process.env.FLOWPAY_API_URL??"https://api.pixuno.xyz").replace(/\/$/,"");
const checkoutBase=(process.env.FLOWPAY_CHECKOUT_URL??"https://checkout.pixuno.xyz").replace(/\/$/,"");
const mcpResource=process.env.FLOWPAY_MCP_RESOURCE_URL??"https://mcp.pixuno.xyz/mcp";
const metadataUrl=new URL("/.well-known/oauth-protected-resource",mcpResource).toString();
const authChallenge=`Bearer resource_metadata="${metadataUrl}", scope="payments:read payments:write"`;

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

function unauthorized(message="Connect your FlowPay account through the authorization page"){
  const headers=new Headers({"content-type":"application/json","www-authenticate":authChallenge});
  cors(headers);
  return new Response(JSON.stringify({error:"unauthorized",message}),{status:401,headers});
}

async function flowpay(key:string,route:string,init:RequestInit={}){
  const headers=new Headers(init.headers);
  if(key.startsWith("fp_")&&!key.startsWith("fp_oauth_"))headers.set("x-flowpay-api-key",key);
  else headers.set("authorization",`Bearer ${key}`);
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

function createServer(key:string,scopes:string[]){
  const server=new McpServer({name:"FlowPay",version:"1.0.0"});
  const security=(required:string[])=>({securitySchemes:[{type:"oauth2",scopes:required}]});
  const checkScope=(required:string)=>{
    if(!scopes.includes(required))return {isError:true,content:[{type:"text" as const,text:"Reconnect FlowPay and approve the requested permission."}],_meta:{"mcp/www_authenticate":authChallenge}};
    return null;
  };

  server.registerTool("flowpay_integration_guide",{
    _meta:security(["payments:read"]),
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
    _meta:security(["payments:read"]),
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
    _meta:security(["payments:read"]),
    title:"Verify FlowPay credentials",
    description:"Tests the connector's stored credential against the live FlowPay API without returning the credential.",
    inputSchema:{},
    annotations:{readOnlyHint:true,destructiveHint:false,idempotentHint:true,openWorldHint:true},
  },async()=>{await flowpay(key,"/v1/payments?limit=1");return result({authenticated:true,api:apiBase,mode:"live",secrets_exposed:false});});

  server.registerTool("flowpay_list_payments",{
    _meta:security(["payments:read"]),
    title:"List FlowPay payments",
    description:"Lists crypto payments belonging to the authenticated FlowPay merchant.",
    inputSchema:{limit:z.number().int().min(1).max(100).default(20)},
    annotations:{readOnlyHint:true,destructiveHint:false,idempotentHint:true,openWorldHint:true},
  },async({limit})=>checkScope("payments:read")??result(await flowpay(key,`/v1/payments?limit=${limit}`)));

  server.registerTool("flowpay_get_payment",{
    _meta:security(["payments:read"]),
    title:"Get a FlowPay payment",
    description:"Returns one crypto payment belonging to the authenticated merchant.",
    inputSchema:{paymentId:z.string().min(1)},
    annotations:{readOnlyHint:true,destructiveHint:false,idempotentHint:true,openWorldHint:true},
  },async({paymentId})=>checkScope("payments:read")??result(await flowpay(key,`/v1/payments/${encodeURIComponent(paymentId)}`)));

  server.registerTool("flowpay_create_payment",{
    _meta:security(["payments:write"]),
    title:"Create a FlowPay crypto payment",
    description:"Creates a live hosted crypto checkout after the user confirms the amount, asset, network, and reference. Fiat creation is not supported.",
    inputSchema:{amount:z.string().regex(/^\d+(\.\d+)?$/),asset:z.enum(["USDC","USDT","ETH"]),chain:z.enum(["base_sepolia","ethereum_sepolia","arbitrum_sepolia","bsc_testnet"]),reference:z.string().max(160).optional(),expiresInSeconds:z.number().int().min(60).max(2592000).default(1800)},
    annotations:{readOnlyHint:false,destructiveHint:false,idempotentHint:false,openWorldHint:true},
  },async({amount,asset,chain,reference,expiresInSeconds})=>checkScope("payments:write")??result(await flowpay(key,"/v1/payments",{method:"POST",headers:{"content-type":"application/json","idempotency-key":crypto.randomUUID()},body:JSON.stringify({amount,asset,chain,reference,expires_in_seconds:expiresInSeconds})})));

  return server;
}

async function handler(request:Request){
  if(request.method==="OPTIONS"){
    const headers=new Headers();cors(headers);return new Response(null,{status:204,headers});
  }
  const key=bearer(request);
  if(!key)return unauthorized();
  let scopes:string[];
  try{
    if(key.startsWith("fp_oauth_")){
      const info=await flowpay(key,"/v1/oauth/token-info") as {active:boolean;issuer:string;audience:string;scope:string};
      if(!info.active||info.audience!==mcpResource||info.issuer!==(process.env.FLOWPAY_OAUTH_ISSUER??"https://api.pixuno.xyz").replace(/\/$/,""))return unauthorized();
      scopes=info.scope.split(/\s+/);
    }else{
      await flowpay(key,"/v1/payments?limit=1");
      scopes=["payments:read","payments:write"];
    }
  }catch{return unauthorized("Your FlowPay connection expired or was revoked. Connect again.");}
  const transport=new WebStandardStreamableHTTPServerTransport({sessionIdGenerator:undefined,enableJsonResponse:true,maxRequestBodySize:1_048_576});
  const server=createServer(key,scopes);
  try{
    await server.connect(transport);
    const response=await transport.handleRequest(request,{authInfo:{token:key,clientId:"flowpay",scopes}});
    cors(response.headers);
    if(response.headers.get("content-type")?.includes("application/json")){
      const body=await response.clone().json().catch(()=>null);
      if(Array.isArray(body?.result?.tools)){
        body.result.tools=body.result.tools.map((tool:Record<string,any>)=>({...tool,securitySchemes:tool._meta?.securitySchemes??[{type:"oauth2",scopes:["payments:read"]}]}));
        return new Response(JSON.stringify(body),{status:response.status,headers:response.headers});
      }
    }
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

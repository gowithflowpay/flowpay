import {createServer} from "node:http";
import {GET as metadata} from "../api/resource-metadata.js";
import {GET as authMetadata} from "../api/authorization-metadata.js";
import {GET,POST,DELETE,OPTIONS} from "../api/mcp.js";

const port=Number(process.env.PORT??3101);
const bind=process.env.HOST??"127.0.0.1";
const limit=1_048_576;
const server=createServer(async(req,res)=>{
  try{
    const resource=process.env.FLOWPAY_MCP_RESOURCE_URL??"https://mcp.pixuno.xyz/mcp";
    const url=new URL(req.url??"/",resource);
    let response:Response;
    if(url.pathname==="/health")response=Response.json({ok:true,service:"flowpay-mcp",authentication:"OAuth 2.1"});
    else if(url.pathname==="/.well-known/oauth-protected-resource"||url.pathname==="/.well-known/oauth-protected-resource/mcp")response=metadata();
    else if(url.pathname==="/.well-known/oauth-authorization-server")response=await authMetadata();
    else if(url.pathname==="/mcp"){
      const chunks:Buffer[]=[];let size=0;
      for await(const chunk of req){size+=chunk.length;if(size>limit){res.writeHead(413);res.end();return;}chunks.push(chunk);}
      const headers=new Headers();for(const [key,value]of Object.entries(req.headers))if(value)headers.set(key,Array.isArray(value)?value.join(","):value);
      const request=new Request(url,{method:req.method,headers,...(chunks.length?{body:Buffer.concat(chunks)}:{})});
      const handler=({GET,POST,DELETE,OPTIONS} as Record<string,(request:Request)=>Promise<Response>>)[req.method??""];
      response=handler?await handler(request):new Response(null,{status:405});
    }else response=Response.json({endpoint:resource,authorization:"OAuth authorization code with PKCE S256"});
    res.writeHead(response.status,Object.fromEntries(response.headers.entries()));
    if(response.body)for await(const chunk of response.body as unknown as AsyncIterable<Uint8Array>)res.write(chunk);
    res.end();
  }catch(error){console.error(error instanceof Error?error.message:"MCP request failed");if(!res.headersSent)res.writeHead(502,{"content-type":"application/json"});res.end(JSON.stringify({error:"service_unavailable"}));}
});
server.headersTimeout=15000;server.requestTimeout=30000;
server.listen(port,bind,()=>console.error(`FlowPay MCP listening on ${bind}:${port}`));

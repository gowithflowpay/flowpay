export function GET(){
  return Response.json({name:"FlowPay MCP",status:"ready",endpoint:"https://mcp.pixuno.xyz/mcp",transport:"Streamable HTTP",authentication:"OAuth authorization code with PKCE S256",resource_metadata:"https://mcp.pixuno.xyz/.well-known/oauth-protected-resource",secrets:"Credentials are never returned in tool output",health:"https://mcp.pixuno.xyz/health"});
}

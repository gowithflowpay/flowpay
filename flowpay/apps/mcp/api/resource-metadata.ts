export function GET(){
  const resource=process.env.FLOWPAY_MCP_RESOURCE_URL??"https://mcp.pixuno.xyz/mcp";
  const issuer=(process.env.FLOWPAY_OAUTH_ISSUER??"https://api.pixuno.xyz").replace(/\/$/,"");
  return Response.json({resource,authorization_servers:[issuer],scopes_supported:["payments:read","payments:write"],bearer_methods_supported:["header"],resource_documentation:"https://github.com/gowithflowpay/flowpay/blob/main/flowpay/apps/mcp/README.md"},{headers:{"access-control-allow-origin":"*","cache-control":"public, max-age=300"}});
}

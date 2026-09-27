export function GET(){
  return Response.json({name:"FlowPay MCP",status:"ready",endpoint:"https://mcp.pixuno.xyz/mcp",transport:"Streamable HTTP",authentication:"FlowPay API key as Bearer token",secrets:"Credentials are read from the connector and never returned in tool output",health:"https://mcp.pixuno.xyz/health"});
}

export function GET(){
  return Response.json({ok:true,service:"flowpay-mcp",transport:"streamable-http",version:"1.0.0",api:"https://api.pixuno.xyz",fiat:false});
}

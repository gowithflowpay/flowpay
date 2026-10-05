export async function GET(){
  const issuer=(process.env.FLOWPAY_OAUTH_ISSUER??"https://api.pixuno.xyz").replace(/\/$/,"");
  const response=await fetch(`${issuer}/.well-known/oauth-authorization-server`,{signal:AbortSignal.timeout(10000)});
  return new Response(await response.text(),{status:response.status,headers:{"content-type":"application/json","access-control-allow-origin":"*","cache-control":"public, max-age=300"}});
}

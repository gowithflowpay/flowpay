export const runtime="nodejs";

export async function POST(request:Request){
  const base=(process.env.FLOWPAY_API_URL??"http://127.0.0.1:8080").replace(/\/$/,"");
  const signature=request.headers.get("x-alchemy-signature");
  if(!signature)return Response.json({error:"missing Alchemy signature"},{status:401});
  const body=await request.text();
  const response=await fetch(`${base}/v1/providers/alchemy/webhook`,{
    method:"POST",
    headers:{"content-type":"application/json","x-alchemy-signature":signature},
    body,
  });
  if(response.status===204)return new Response(null,{status:204});
  const text=await response.text();
  return new Response(text,{status:response.status,headers:{"content-type":response.headers.get("content-type")??"application/json"}});
}

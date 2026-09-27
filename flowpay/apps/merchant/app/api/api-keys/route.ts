import {NextRequest,NextResponse} from "next/server";
import {api} from "../../../lib/api";

export async function POST(request:NextRequest){
  try{
    const body=await request.json();
    const created=await api("/v1/api-keys",{method:"POST",body:JSON.stringify(body)});
    const apiBase=(process.env.FLOWPAY_API_URL??"http://127.0.0.1:8080").replace(/\/$/,"");
    const verification=await fetch(`${apiBase}/v1/payments?limit=1`,{headers:{"x-flowpay-api-key":created.api_key},cache:"no-store"});
    if(!verification.ok){
      await api(`/v1/api-keys/${encodeURIComponent(created.id)}/revoke`,{method:"POST"}).catch(()=>undefined);
      return NextResponse.json({error:{message:"FlowPay created the key but live authentication failed; it was revoked."}},{status:502});
    }
    return NextResponse.json({...created,verified:true},{status:201});
  }
  catch(error){return NextResponse.json({error:{message:error instanceof Error?error.message:"Could not generate API key"}},{status:500});}
}

export async function DELETE(request:NextRequest){
  const id=request.nextUrl.searchParams.get("id");
  if(!id)return NextResponse.json({error:"API key id is required"},{status:400});
  try{return NextResponse.json(await api(`/v1/api-keys/${encodeURIComponent(id)}/revoke`,{method:"POST"}));}
  catch(error){return NextResponse.json({error:{message:error instanceof Error?error.message:"Could not revoke API key"}},{status:500});}
}

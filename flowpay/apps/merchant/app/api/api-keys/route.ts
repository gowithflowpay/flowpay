import {NextRequest,NextResponse} from "next/server";
import {api} from "../../../lib/api";

export async function POST(request:NextRequest){
  try{const body=await request.json();return NextResponse.json(await api("/v1/api-keys",{method:"POST",body:JSON.stringify(body)}),{status:201});}
  catch(error){return NextResponse.json({error:{message:error instanceof Error?error.message:"Could not generate API key"}},{status:500});}
}

export async function DELETE(request:NextRequest){
  const id=request.nextUrl.searchParams.get("id");
  if(!id)return NextResponse.json({error:"API key id is required"},{status:400});
  try{return NextResponse.json(await api(`/v1/api-keys/${encodeURIComponent(id)}/revoke`,{method:"POST"}));}
  catch(error){return NextResponse.json({error:{message:error instanceof Error?error.message:"Could not revoke API key"}},{status:500});}
}

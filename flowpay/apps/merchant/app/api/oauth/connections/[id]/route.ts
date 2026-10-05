import {NextRequest,NextResponse} from "next/server";
import {api,ApiError} from "../../../../../lib/api";
export async function DELETE(request:NextRequest,{params}:{params:Promise<{id:string}>}){
  if(request.headers.get("origin")!==request.nextUrl.origin)return NextResponse.json({error:{message:"Invalid request origin"}},{status:403});
  const {id}=await params;
  if(!/^[0-9a-f-]{36}$/i.test(id))return NextResponse.json({error:{message:"Invalid connection"}},{status:400});
  try{await api(`/v1/oauth/connections/${id}/revoke`,{method:"POST"});return NextResponse.json({revoked:true});}
  catch(error){return NextResponse.json({error:{message:error instanceof Error?error.message:"Unable to disconnect"}},{status:error instanceof ApiError?error.status:502});}
}

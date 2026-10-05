import {NextRequest,NextResponse} from "next/server";
import {api,ApiError} from "../../../../../lib/api";

export async function POST(request:NextRequest,{params}:{params:Promise<{id:string}>}){
  if(request.headers.get("origin")!==request.nextUrl.origin)return NextResponse.json({error:{message:"Invalid request origin"}},{status:403});
  const {id}=await params;
  if(!/^[0-9a-f-]{36}$/i.test(id))return NextResponse.json({error:{message:"Invalid connection request"}},{status:400});
  try{
    const input=await request.json();
    if(typeof input.approved!=="boolean")return NextResponse.json({error:{message:"Choose whether to authorize this app"}},{status:400});
    return NextResponse.json(await api(`/v1/oauth/requests/${id}`,{method:"POST",body:JSON.stringify({approved:input.approved})}));
  }catch(error){return NextResponse.json({error:{message:error instanceof Error?error.message:"Unable to connect"}},{status:error instanceof ApiError?error.status:502});}
}

import {NextResponse} from "next/server";
import {api} from "../../../../../lib/api";

export const dynamic="force-dynamic";

export async function POST(request:Request,{params}:{params:Promise<{id:string}>}){
  try{
    const {id}=await params;
    const body=await request.json();
    // Forward to backend agent chat endpoint
    const result=await api(`/v1/agent/chat`,{
      method:"POST",
      body:JSON.stringify({...body,payment_id:id}),
    });
    return NextResponse.json(result);
  }catch(e){
    return NextResponse.json({error:e instanceof Error?e.message:"Agent chat failed"},{status:502});
  }
}

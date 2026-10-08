import {NextResponse} from "next/server";
import {api} from "../../../../../../lib/api";
export const dynamic="force-dynamic";
export async function GET(request:Request,{params}:{params:Promise<{id:string}>}){
 try{const {id}=await params;const query=new URL(request.url).searchParams;const claim=query.get("claim_id");const session=query.get("session_id");if(!claim||!session)return NextResponse.json({error:"Conversation details are required"},{status:400});const search=new URLSearchParams({claim_id:claim,session_id:session});return NextResponse.json(await api(`/v1/public/payments/${encodeURIComponent(id)}/agent/status?${search}`));}
 catch{return NextResponse.json({error:"Investigation status is temporarily unavailable"},{status:502});}
}

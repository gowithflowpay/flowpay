import {NextResponse} from "next/server";
import {api} from "../../../../../lib/api";
export async function POST(request:Request,{params}:{params:Promise<{id:string}>}){try{const {id}=await params;const body=await request.json();return NextResponse.json(await api(`/v1/claims/${encodeURIComponent(id)}/authorize`,{method:"POST",body:JSON.stringify(body)}));}catch(e){return NextResponse.json({error:e instanceof Error?e.message:"Unable to verify wallet"},{status:502});}}

import {NextResponse} from "next/server";
import {api} from "../../../lib/api";
export async function POST(request:Request){try{const body=await request.json();const result=await api("/v1/claims",{method:"POST",headers:{"idempotency-key":crypto.randomUUID()},body:JSON.stringify(body)});return NextResponse.json(result,{status:201});}catch(e){return NextResponse.json({error:e instanceof Error?e.message:"Unable to create claim"},{status:502});}}

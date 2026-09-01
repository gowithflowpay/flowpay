import {NextResponse} from "next/server";
import {api} from "../../../lib/api";

export const dynamic="force-dynamic";

export async function GET(){
  try{return NextResponse.json(await api("/v1/webhooks/deliveries"),{headers:{"cache-control":"no-store"}})}
  catch(error){return NextResponse.json({error:{message:error instanceof Error?error.message:"Unable to load notifications"}},{status:502})}
}

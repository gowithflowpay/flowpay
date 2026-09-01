import {NextResponse} from "next/server";
import {api} from "../../../../../lib/api";
export const dynamic="force-dynamic";
function noStore(value:unknown,status=200){const response=NextResponse.json(value,{status});response.headers.set("Cache-Control","private, no-store, no-cache, must-revalidate, max-age=0");return response;}
export async function GET(_:Request,{params}:{params:Promise<{id:string}>}){try{const {id}=await params;return noStore(await api(`/v1/payments/${encodeURIComponent(id)}/deposits`));}catch(e){return noStore({error:e instanceof Error?e.message:"Unable to load deposits"},502);}}

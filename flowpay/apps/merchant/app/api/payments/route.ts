import {NextResponse} from "next/server";
import {api} from "../../../lib/api";

export async function POST(request:Request){
  try{
    const formData=await request.formData();
    const result=await api("/v1/payments",{
      method:"POST",
      headers:{"idempotency-key":crypto.randomUUID()},
      body:JSON.stringify({
        amount:String(formData.get("amount")??""),
        asset:String(formData.get("asset")??"USDC"),
        chain:String(formData.get("chain")??"base_sepolia"),
        reference:String(formData.get("reference")??"")||undefined,
      }),
    });
    if(request.headers.get("accept")?.includes("application/json"))return NextResponse.json(result,{status:201});
    return NextResponse.redirect(new URL(`/payments/${encodeURIComponent(result.id)}`,request.url),303);
  }catch(error){
    const message=error instanceof Error?error.message:"Unable to create payment";
    return NextResponse.json({error:{message}},{status:502});
  }
}

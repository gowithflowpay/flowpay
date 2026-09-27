import {NextResponse} from "next/server";
import {api} from "../../../lib/api";

export async function POST(request:Request){
  try{
    const formData=await request.formData();
    const amount=String(formData.get("amount")??"").trim();
    if(!/^\d+(?:\.\d+)?$/.test(amount)||Number(amount)<=0){
      return NextResponse.json({error:{message:"Enter an amount greater than zero."}},{status:400});
    }
    const expiryDays=Number(String(formData.get("expiry")??"7"));
    const result=await api("/v1/payments",{
      method:"POST",
      headers:{"idempotency-key":crypto.randomUUID()},
      body:JSON.stringify({
        amount,
        asset:String(formData.get("asset")??"USDC"),
        chain:String(formData.get("chain")??"base_sepolia"),
        reference:String(formData.get("reference")??"")||undefined,
        expires_in_seconds:Math.round(Math.min(30,Math.max(1,Number.isFinite(expiryDays)?expiryDays:7))*86400),
      }),
    });
    const requestUrl=new URL(request.url);
    if(["localhost","127.0.0.1"].includes(requestUrl.hostname)&&result?.id){
      result.checkout_url=`http://localhost:3001/pay/${encodeURIComponent(result.id)}`;
    }
    if(request.headers.get("accept")?.includes("application/json"))return NextResponse.json(result,{status:201});
    return NextResponse.redirect(new URL(`/payments?created=${encodeURIComponent(result.id)}`,request.url),303);
  }catch(error){
    const message=error instanceof Error?error.message:"Unable to create payment";
    return NextResponse.json({error:{message}},{status:502});
  }
}

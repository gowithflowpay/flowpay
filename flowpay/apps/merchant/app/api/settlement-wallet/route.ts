import {NextResponse} from "next/server";
import {api,ApiError} from "../../../lib/api";

export async function POST(request:Request){
 try{
  const origin=request.headers.get("origin");const host=request.headers.get("x-forwarded-host")??request.headers.get("host")??new URL(request.url).host;
  if(origin&&new URL(origin).host!==host)return NextResponse.json({error:{message:"Invalid request origin."}},{status:403});
  const input=await request.json();const address=typeof input.address==="string"?input.address.trim():"";
  if(!/^0x[0-9a-fA-F]{40}$/.test(address)||/^0x0{40}$/i.test(address))return NextResponse.json({error:{message:"Enter a valid, non-zero EVM wallet address."}},{status:400});
  const result=await api("/v1/merchant/settlement-wallet",{method:"POST",body:JSON.stringify({address})});
  return NextResponse.json(result,{headers:{"Cache-Control":"no-store"}});
 }catch(error){
  const status=error instanceof ApiError?error.status:error instanceof SyntaxError?400:502;
  const message=status===401||status===403?"Your session has ended. Sign in again.":status===409?"A settlement wallet is already configured. Refresh this page.":status===400?"Enter a valid EVM wallet address.":"Wallet settings are temporarily unavailable. Try again.";
  return NextResponse.json({error:{message}},{status});
 }
}

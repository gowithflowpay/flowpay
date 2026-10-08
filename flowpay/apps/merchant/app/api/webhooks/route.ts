import {NextResponse} from "next/server";
import {api,ApiError} from "../../../lib/api";
export async function POST(request:Request){
 try{const origin=request.headers.get("origin");const host=request.headers.get("x-forwarded-host")??request.headers.get("host")??new URL(request.url).host;if(origin&&new URL(origin).host!==host)return NextResponse.json({error:{message:"Invalid request origin."}},{status:403});
 const body=await request.json();if(body.action==="test")return NextResponse.json(await api("/v1/webhooks/test",{method:"POST"}));
 if(body.action!=="create"||typeof body.url!=="string")return NextResponse.json({error:{message:"Provide an endpoint URL."}},{status:400});
 const url=new URL(body.url.trim());if(url.protocol!=="https:"||url.username||url.password||url.hash)return NextResponse.json({error:{message:"Use a valid HTTPS endpoint URL."}},{status:400});
 const result=await api("/v1/webhooks",{method:"POST",body:JSON.stringify({url:url.toString(),events:[]})});return NextResponse.json(result,{status:201,headers:{"Cache-Control":"no-store"}});
 }catch(error){const status=error instanceof ApiError?error.status:error instanceof SyntaxError||error instanceof TypeError?400:502;return NextResponse.json({error:{message:status===401?"Your session has ended. Sign in again.":status===400?"Enter a valid public HTTPS endpoint URL.":"Webhook settings are temporarily unavailable. Try again."}},{status});}
}

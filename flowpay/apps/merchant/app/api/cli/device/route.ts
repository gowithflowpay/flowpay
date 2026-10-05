import {NextRequest,NextResponse} from "next/server";
import {api} from "../../../../lib/api";

export async function POST(request:NextRequest){
  try{
    const body=await request.json();
    const decision=body?.decision;
    const userCode=typeof body?.user_code==="string"?body.user_code.trim().toUpperCase():"";
    if(decision!=="approve"&&decision!=="deny"){
      return NextResponse.json({error:{message:"Choose whether to approve or deny the device."}},{status:400});
    }
    if(!/^[ABCDEFGHJKMNPQRSTVWXYZ2-9]{4}-[ABCDEFGHJKMNPQRSTVWXYZ2-9]{4}$/.test(userCode)){
      return NextResponse.json({error:{message:"Enter the 8-character code shown in the terminal."}},{status:400});
    }
    const result=await api(`/v1/cli/device/${decision}`,{
      method:"POST",
      body:JSON.stringify({user_code:userCode}),
    });
    return NextResponse.json(result,{status:decision==="approve"?201:200});
  }catch(error){
    const status=typeof error==="object"&&error!==null&&"status" in error&&typeof error.status==="number"?error.status:500;
    return NextResponse.json({error:{message:error instanceof Error?error.message:"Could not process this device request."}},{status});
  }
}

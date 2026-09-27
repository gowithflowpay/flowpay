import {NextResponse} from "next/server";
import {api} from "../../../../../../lib/api";

export const dynamic="force-dynamic";

export async function GET(request:Request){
  try{
    const claimId=new URL(request.url).searchParams.get("claim_id");
    if(!claimId)return NextResponse.json({error:"claim_id is required"},{status:400});
    const claim=await api(`/v1/claims/${encodeURIComponent(claimId)}`);
    const run=Array.isArray(claim?.agent?.runs)?claim.agent.runs.at(-1):null;
    const verdict=claim.status==="RECOVERED"
      ?"RECOVERED"
      :claim.status==="RECOVERY_PENDING"
        ?"RECOVERY_PENDING"
        :run?.final_disposition??null;
    return NextResponse.json({
      claim_id:claim.id,
      status:claim.status,
      verdict,
      investigation:claim.investigation??null,
      recovery:claim.recovery??null,
    });
  }catch(e){
    return NextResponse.json({error:e instanceof Error?e.message:"Unable to load investigation"},{status:502});
  }
}

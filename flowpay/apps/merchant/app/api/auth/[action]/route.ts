import {NextRequest,NextResponse} from "next/server";
import {ApiError,apiPublic,apiWithToken} from "../../../../lib/api";
import {ONBOARDED_COOKIE,SESSION_COOKIE,SESSION_MAX_AGE} from "../../../../lib/cookies";

// One route for every identity action, so the session cookie is written in
// exactly one place and never touches the browser's JavaScript.
const COOKIE_OPTIONS={
  httpOnly:true,
  sameSite:"lax" as const,
  secure:process.env.NODE_ENV==="production",
  path:"/",
  maxAge:SESSION_MAX_AGE,
};

function attachSession(response:NextResponse,session:any){
  response.cookies.set(SESSION_COOKIE,String(session.session_token),COOKIE_OPTIONS);
  response.cookies.set(
    ONBOARDED_COOKIE,
    session?.merchant?.onboarding_completed?"1":"0",
    COOKIE_OPTIONS,
  );
  return response;
}

function failure(error:unknown){
  if(error instanceof ApiError){
    return NextResponse.json({error:{message:error.message,code:error.code}},{status:error.status});
  }
  const message=error instanceof Error?error.message:"Something went wrong";
  return NextResponse.json({error:{message}},{status:502});
}

async function body(request:NextRequest){
  try{return await request.json();}catch{return {};}
}

export async function POST(
  request:NextRequest,
  context:{params:Promise<{action:string}>},
){
  const {action}=await context.params;
  try{
    if(action==="logout"){
      const token=request.cookies.get(SESSION_COOKIE)?.value;
      if(token){
        // Best effort: clearing the browser cookie is what signs the user out.
        try{await apiWithToken("/v1/auth/logout",{method:"POST"},token);}catch{}
      }
      const response=NextResponse.json({signed_out:true});
      response.cookies.delete(SESSION_COOKIE);
      response.cookies.delete(ONBOARDED_COOKIE);
      return response;
    }

    if(action==="signup"){
      const input=await body(request);
      const data=await apiPublic("/v1/auth/signup",{method:"POST",body:JSON.stringify(input)});
      return NextResponse.json(
        {email:data.email,verification_required:true,email_sent:data.email_sent!==false},
        {status:data.email_sent===false?202:201},
      );
    }

    if(action==="resend"){
      const input=await body(request);
      const data=await apiPublic("/v1/auth/resend",{method:"POST",body:JSON.stringify(input)});
      return NextResponse.json({sent:data.sent!==false});
    }

    if(action==="verify"){
      const input=await body(request);
      const data=await apiPublic("/v1/auth/verify",{method:"POST",body:JSON.stringify(input)});
      return attachSession(NextResponse.json({merchant:data.merchant}),data);
    }

    if(action==="login"){
      const input=await body(request);
      const data=await apiPublic("/v1/auth/login",{method:"POST",body:JSON.stringify(input)});
      return attachSession(NextResponse.json({merchant:data.merchant}),data);
    }

    if(action==="onboarding"){
      const input=await body(request);
      const token=request.cookies.get(SESSION_COOKIE)?.value;
      if(!token)return NextResponse.json({error:{message:"Please sign in again."}},{status:401});
      const data=await apiWithToken(
        "/v1/auth/onboarding",
        {method:"POST",body:JSON.stringify(input)},
        token,
      );
      const response=NextResponse.json({merchant:data.merchant,api_key:data.api_key??null});
      response.cookies.set(ONBOARDED_COOKIE,"1",COOKIE_OPTIONS);
      return response;
    }

    // Unknown action: behave like a stub, never like an open proxy.
    return NextResponse.json({error:{message:"Unknown action"}},{status:404});
  }catch(error){
    return failure(error);
  }
}

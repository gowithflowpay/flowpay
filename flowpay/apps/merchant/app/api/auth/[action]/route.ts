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
    const passkeyActions:Record<string,string>={"passkey-signup-start":"signup/start","passkey-signup-finish":"signup/finish","passkey-login-start":"discover/start","passkey-login-finish":"discover/finish","passkey-register-start":"register/start","passkey-register-finish":"register/finish"};
    if(action in passkeyActions){
      const input=await body(request);
      const path="/v1/auth/passkeys/"+passkeyActions[action];
      let data:any;
      if(action.startsWith("passkey-register")){
        const token=request.cookies.get(SESSION_COOKIE)?.value;
        if(!token)return NextResponse.json({error:{message:"Please sign in again."}},{status:401});
        data=await apiWithToken(path,{method:"POST",body:JSON.stringify(input)},token);
      }else data=await apiPublic(path,{method:"POST",body:JSON.stringify(input)});
      if(data.session_token)return attachSession(NextResponse.json({merchant:data.merchant}),data);
      return NextResponse.json(data);
    }
    if(["signup","login","verify","resend"].includes(action))return NextResponse.json({error:{message:"Sign in with a passkey. Passwords and email codes are no longer used for browser sign-in."}},{status:410});
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

    if(action==="onboarding"){
      const input=await body(request);
      input.require_passkey=true;
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

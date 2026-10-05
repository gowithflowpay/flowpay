import {redirect} from "next/navigation";
import {api} from "../../../lib/api";
import {getMerchant} from "../../../lib/session";
import {ConsentPanel} from "./ConsentPanel";

export const dynamic="force-dynamic";
export default async function AuthorizePage({searchParams}:{searchParams:Promise<{request?:string}>}){
  const {request}=await searchParams;
  if(!request||!/^[0-9a-f-]{36}$/i.test(request))return <div className="auth-card"><h1>Connection unavailable</h1><p>Start the connection again from ChatGPT or Claude.</p></div>;
  const merchant=await getMerchant();
  if(!merchant)redirect(`/login?${new URLSearchParams({next:`/oauth/authorize?request=${request}`}).toString()}`);
  try{
    const details=await api(`/v1/oauth/requests/${request}`);
    return <ConsentPanel requestId={request} clientName={details.client_name} callbackHost={new URL(details.redirect_uri).hostname} scopes={details.scopes} email={merchant.email??"Your account"}/>;
  }catch{
    return <div className="auth-card"><h1>This request has expired</h1><p>Return to ChatGPT or Claude and connect FlowPay again.</p><a className="btn primary" href="/dashboard">Go to dashboard</a></div>;
  }
}

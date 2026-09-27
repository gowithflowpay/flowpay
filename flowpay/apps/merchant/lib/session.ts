import "server-only";
import {redirect} from "next/navigation";
import {api} from "./api";

export type Merchant={
  id:string;
  email:string|null;
  business_name:string;
  contact_name:string|null;
  public_id:string;
  status:string;
  email_verified:boolean;
  onboarding_completed:boolean;
  settlement_address:string|null;
  created_at:number;
};

export type Session={
  session_token:string;
  expires_at:number;
  merchant:Merchant;
};

/** Resolves the current merchant, or null when the session is missing/expired. */
export async function getMerchant():Promise<Merchant|null>{
  try{
    const data=await api("/v1/auth/session");
    return (data?.merchant as Merchant)??null;
  }catch{
    return null;
  }
}

/** Same as {@link getMerchant}, but sends the visitor to sign in when absent. */
export async function requireMerchant():Promise<Merchant>{
  const merchant=await getMerchant();
  if(!merchant)redirect("/login");
  return merchant;
}

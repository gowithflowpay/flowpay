import "server-only";
import {cookies} from "next/headers";
import {SESSION_COOKIE} from "./cookies";

/** An error carrying the API's own status so callers can react to 401s. */
export class ApiError extends Error{
  status:number;
  code?:string;
  constructor(message:string,status:number,code?:string){
    super(message);
    this.name="ApiError";
    this.status=status;
    this.code=code;
  }
}

function apiBase(){
  return (process.env.FLOWPAY_API_URL??"http://127.0.0.1:8080").replace(/\/$/,"");
}

async function readBody(response:Response){
  const text=await response.text();
  try{return text?JSON.parse(text):{};}catch{return {raw:text};}
}

async function send(pathname:string,init:RequestInit,token:string|null){
  const headers=new Headers(init.headers as HeadersInit|undefined);
  headers.set("content-type","application/json");
  if(token)headers.set("authorization",`Bearer ${token}`);
  const response=await fetch(apiBase()+pathname,{...init,cache:"no-store",headers});
  const data=await readBody(response);
  if(!response.ok){
    const error=data?.error;
    throw new ApiError(error?.message??`FlowPay API ${response.status}`,response.status,error?.code);
  }
  return data;
}

/** The signed-in merchant's session token, or null when signed out. */
export async function sessionToken(){
  const store=await cookies();
  return store.get(SESSION_COOKIE)?.value??null;
}

/** Calls the FlowPay API as the merchant holding the current session. */
export async function api(pathname:string,init:RequestInit={}){
  const token=await sessionToken();
  if(!token)throw new ApiError("Your session has ended. Please sign in again.",401,"no_session");
  return send(pathname,init,token);
}

/** Calls an endpoint that does not require a session (signup, login, verify). */
export async function apiPublic(pathname:string,init:RequestInit={}){
  return send(pathname,init,null);
}

/** Like {@link apiPublic}, but authenticates with an explicit bearer token. */
export async function apiWithToken(pathname:string,init:RequestInit,token:string){
  return send(pathname,init,token);
}

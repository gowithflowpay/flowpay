import "server-only";
const base=(process.env.FLOWPAY_API_URL??"http://127.0.0.1:8080").replace(/\/$/,"");
const key=process.env.FLOWPAY_API_KEY??"";
export async function api(path:string,init:RequestInit={}){
  const h=new Headers(init.headers as HeadersInit|undefined);
  h.set("content-type","application/json");
  if(key)h.set("x-flowpay-api-key",key);
  const r=await fetch(base+path,{...init,cache:"no-store",headers:h});
  const text=await r.text();
  let data:any={};
  try{data=text?JSON.parse(text):{};}catch{data={raw:text};}
  if(!r.ok)throw new Error(data?.error?.message??`FlowPay API ${r.status}`);
  return data;
}

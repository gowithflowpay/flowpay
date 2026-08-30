import "server-only";
const base=(process.env.FLOWPAY_API_URL??"http://127.0.0.1:8080").replace(/\/$/,"");
const key=process.env.FLOWPAY_API_KEY??"";
export async function api(path:string,init:RequestInit={}){
  if(!key)throw new Error("FLOWPAY_API_KEY is not configured. Set it in your .env file.");
  const r=await fetch(base+path,{...init,cache:"no-store",headers:{"x-flowpay-api-key":key,"content-type":"application/json",...(init.headers??{})}});
  const text=await r.text();
  let data:any={};
  try{data=text?JSON.parse(text):{};}catch{data={raw:text};}
  if(!r.ok)throw new Error(data?.error?.message??`FlowPay API ${r.status}`);
  return data;
}

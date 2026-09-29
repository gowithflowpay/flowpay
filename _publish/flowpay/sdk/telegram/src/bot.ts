import {createServer, type IncomingMessage} from "node:http";
import {FlowPay, verifyWebhookSignature} from "@flowpay/node";

const token=must("TELEGRAM_BOT_TOKEN");
const flowpay=new FlowPay({apiKey:must("FLOWPAY_API_KEY"),baseUrl:process.env.FLOWPAY_API_URL??"http://127.0.0.1:8080"});
const telegram=`https://api.telegram.org/bot${token}`;
const paymentChats=new Map<string,number>();
let offset=0;

async function tg(method:string,body:unknown){const r=await fetch(`${telegram}/${method}`,{method:"POST",headers:{"content-type":"application/json"},body:JSON.stringify(body)});if(!r.ok)throw new Error(`Telegram ${method} failed: ${r.status}`);return r.json();}
async function message(chat_id:number,text:string){await tg("sendMessage",{chat_id,text,disable_web_page_preview:true});}
async function handleCommand(chatId:number,text:string){
  if(text.trim()!=="/buy")return message(chatId,"Use /buy to purchase the demo product for 50 USDC on Base.");
  const payment=await flowpay.payments.create({amount:"50",asset:"USDC",chain:"base",reference:`TG_${chatId}_${Date.now()}`});
  paymentChats.set(payment.id,chatId);
  await message(chatId,["FlowPay demo order","","Amount: 50 USDC","Network: Base",`Address: ${payment.address}`,`Payment ID: ${payment.id}`,"","The product activates only after FlowPay sends payment.completed."].join("\n"));
}
async function poll(){for(;;){try{const r:any=await tg("getUpdates",{offset,timeout:25,allowed_updates:["message"]});for(const u of r.result??[]){offset=Math.max(offset,u.update_id+1);const m=u.message;if(m?.text&&m?.chat?.id)await handleCommand(m.chat.id,m.text);}}catch(e){console.error("telegram poll error",e);await new Promise(r=>setTimeout(r,1500));}}}
async function readBody(req:IncomingMessage):Promise<Uint8Array>{const parts:Uint8Array[]=[];for await(const chunk of req)parts.push(typeof chunk==="string"?new TextEncoder().encode(chunk):chunk);const size=parts.reduce((n,p)=>n+p.length,0);const out=new Uint8Array(size);let i=0;for(const p of parts){out.set(p,i);i+=p.length;}return out;}
function webhookServer(){const secret=process.env.FLOWPAY_WEBHOOK_SECRET;if(!secret){console.warn("FLOWPAY_WEBHOOK_SECRET not set; activation webhook listener disabled");return;}const port=Number(process.env.PORT??8091);createServer(async(req,res)=>{if(req.method!=="POST"||req.url!=="/flowpay/webhook"){res.statusCode=404;return res.end();}const raw=await readBody(req);const signature=String(req.headers["flowpay-signature"]??"");if(!await verifyWebhookSignature(raw,signature,secret)){res.statusCode=401;return res.end("invalid signature");}const event=JSON.parse(new TextDecoder().decode(raw));if(event.type==="payment.completed"){const paymentId=event.data?.id??event.aggregate_public_id;const chat=paymentChats.get(paymentId);if(chat){await message(chat,"✅ Payment completed. Your demo product is now active.");paymentChats.delete(paymentId);}}res.statusCode=204;res.end();}).listen(port,"127.0.0.1",()=>console.log(`FlowPay webhook listener on http://127.0.0.1:${port}/flowpay/webhook`));}
function must(k:string){const v=process.env[k];if(!v)throw new Error(`${k} is required`);return v;}
webhookServer();void poll();

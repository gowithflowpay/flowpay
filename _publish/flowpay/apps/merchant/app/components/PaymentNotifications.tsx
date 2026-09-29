"use client";
import {useCallback,useEffect,useRef,useState} from "react";
import {BellIcon,CheckIcon,CopyIcon} from "./Icons";

type Delivery={id:string;event_id:string;event:string;payment_id:string;endpoint:string;response_status:number;delivered_at:string|null;amount_atomic:string|null;asset_decimals:number|null;asset:string|null;chain:string|null};

const seenKey="flowpay-seen-webhook-deliveries";
function amount(value:string|null,decimals:number|null){
  if(!value)return "0";const places=decimals??0;const raw=value.replace(/^0+/,"")||"0";
  if(!places)return raw;const padded=raw.padStart(places+1,"0");const whole=padded.slice(0,-places);const fraction=padded.slice(-places).replace(/0+$/,"");return fraction?`${whole}.${fraction}`:whole;
}
function chainLabel(chain:string|null){return (chain??"Unknown").replace(/_sepolia$/i,"").replace(/_/g," ").replace(/\b\w/g,c=>c.toUpperCase())}

export function PaymentNotifications(){
  const [delivery,setDelivery]=useState<Delivery|null>(null);const [unread,setUnread]=useState(false);const initialized=useRef(false);
  const poll=useCallback(async()=>{try{const response=await fetch("/api/notifications",{cache:"no-store"});if(!response.ok)return;const result=await response.json();const latest:Delivery|undefined=result?.data?.[0];if(!latest)return;
    let seen:string[]=[];try{seen=JSON.parse(localStorage.getItem(seenKey)??"[]")}catch{}
    if(!seen.includes(latest.id)){setUnread(true);if(initialized.current)setDelivery(latest)}initialized.current=true;
  }catch{}},[]);
  useEffect(()=>{void poll();const timer=window.setInterval(()=>void poll(),5000);return()=>window.clearInterval(timer)},[poll]);
  function dismiss(){if(delivery){let seen:string[]=[];try{seen=JSON.parse(localStorage.getItem(seenKey)??"[]")}catch{}localStorage.setItem(seenKey,JSON.stringify([delivery.id,...seen].slice(0,100)))}setDelivery(null);setUnread(false)}
  async function copy(value:string){await navigator.clipboard?.writeText(value)}
  return <><button className="notification-button" aria-label="Notifications" onClick={()=>void poll()}><BellIcon/>{unread?<i/>:null}</button>{delivery?<div className="received-backdrop" onMouseDown={event=>{if(event.target===event.currentTarget)dismiss()}}><section className="received-modal" role="dialog" aria-modal="true" aria-labelledby="payment-received-title"><button className="received-close" onClick={dismiss} aria-label="Close">×</button><div className="received-confetti"><i/><i/><i/><i/><span><CheckIcon/></span></div><h2 id="payment-received-title">Payment received</h2><p>{amount(delivery.amount_atomic,delivery.asset_decimals)} {delivery.asset??""} was received successfully.<br/>FlowPay sent the webhook to your endpoint.</p><div className="received-details"><div><b>⚡</b><span>Event</span><strong>{delivery.event}</strong></div><div><b>◎</b><span>Delivery</span><strong className="delivery-ok">{delivery.response_status} OK</strong></div><div><b>$</b><span>Amount</span><strong>{amount(delivery.amount_atomic,delivery.asset_decimals)} {delivery.asset} on <em>{chainLabel(delivery.chain)}</em></strong></div><div><b>◇</b><span>Payment ID</span><strong><code>{delivery.payment_id}</code><button onClick={()=>void copy(delivery.payment_id)} aria-label="Copy payment ID"><CopyIcon/></button></strong></div><div><b>⌘</b><span>Endpoint</span><strong><code>{delivery.endpoint}</code><button onClick={()=>void copy(delivery.endpoint)} aria-label="Copy endpoint"><CopyIcon/></button></strong></div></div><div className="received-actions"><a href={`/payments/${delivery.payment_id}`} onClick={dismiss}>View payment</a><a href={`/webhooks?event=${encodeURIComponent(delivery.event_id)}`} onClick={dismiss}>View webhook log</a></div></section></div>:null}</>
}

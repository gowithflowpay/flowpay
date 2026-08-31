"use client";

import {useState} from "react";

export function ShareButton({url}:{url:string}){
  const [shared,setShared]=useState(false);

  async function share(){
    if(typeof navigator!=="undefined"&&typeof navigator.share==="function"){
      await navigator.share({title:"FlowPay checkout",text:"Complete your payment securely:",url});
    }else if(typeof navigator!=="undefined"){
      await navigator.clipboard.writeText(url);
    }
    setShared(true);
    window.setTimeout(()=>setShared(false),1600);
  }

  return <button type="button" className="btn secondary" onClick={()=>void share()}>{shared?"Ready to send":"Share checkout link"}</button>;
}

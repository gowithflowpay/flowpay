"use client";

import {useState} from "react";
import {CopyIcon} from "./Icons";

export function CopyAddress({value}:{value:string}){
  const [copied,setCopied]=useState(false);

  async function copy(){
    await navigator.clipboard.writeText(value);
    setCopied(true);
    window.setTimeout(()=>setCopied(false),1400);
  }

  return <button type="button" onClick={()=>void copy()} aria-label="Copy payment address">
    <CopyIcon/>{copied?"Copied":"Copy"}
  </button>;
}

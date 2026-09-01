"use client";
import {useState} from "react";
import {CheckIcon,CopyIcon} from "./Icons";

export function CopyButton({value,label="Copy",iconOnly=false}:{value:string;label?:string;iconOnly?:boolean}){
  const [copied,setCopied]=useState(false);
  async function copy(){await navigator.clipboard.writeText(value);setCopied(true);window.setTimeout(()=>setCopied(false),1600)}
  return <button type="button" className={`copy-button${iconOnly?" icon-only":""}`} aria-label={copied?"Copied":"Copy payment ID"} title={copied?"Copied":"Copy payment ID"} onClick={()=>void copy()}>{copied?<CheckIcon/>:iconOnly?<CopyIcon/>:label}</button>;
}

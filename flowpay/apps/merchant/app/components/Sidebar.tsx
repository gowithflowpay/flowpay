"use client";
import {useEffect,useState} from "react";
import {usePathname} from "next/navigation";
import {MenuIcon,XIcon} from "./Icons";
const items=[["/","Dashboard"],["/payments","Payments"],["/claims","Transfers"],["/settings","Settings"]] as const;
export function Sidebar(){
  const pathname=usePathname();
  const [open,setOpen]=useState(false);
  useEffect(()=>setOpen(false),[pathname]);
  useEffect(()=>{
    const close=()=>setOpen(false);
    window.addEventListener("resize",close);
    return()=>window.removeEventListener("resize",close);
  },[]);
  return <>
    <button className="mobile-menu-button" type="button" aria-label={open?"Close navigation":"Open navigation"} aria-expanded={open} aria-controls="primary-navigation" onClick={()=>setOpen(value=>!value)}>{open?<XIcon/>:<MenuIcon/>}</button>
    <nav id="primary-navigation" className={`primary-nav${open?" open":""}`} aria-label="Primary">{items.map(([href,label])=>{const active=href==="/"?pathname===href:href==="/payments"?pathname.startsWith("/payments"):pathname.startsWith(href);return <a key={href} href={href} className={active?"active":""} onClick={()=>setOpen(false)}>{label}</a>})}</nav>
  </>;
}

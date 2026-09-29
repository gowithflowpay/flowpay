"use client";
import {Dialog} from "@base-ui/react/dialog";
import {useEffect,useState} from "react";
import {usePathname} from "next/navigation";
import {MenuIcon,XIcon} from "./Icons";
const items=[["/","Dashboard"],["/payments","Payments"],["/claims","Claim"],["/developers","API Keys"],["/settings","Settings"]] as const;
export function Sidebar(){
  const pathname=usePathname();
  const [open,setOpen]=useState(false);
  useEffect(()=>setOpen(false),[pathname]);
  useEffect(()=>{
    const close=()=>setOpen(false);
    window.addEventListener("resize",close);
    return()=>window.removeEventListener("resize",close);
  },[]);
  return <Dialog.Root open={open} onOpenChange={setOpen}>
    <nav className="primary-nav" aria-label="Primary navigation">
      {items.map(([href,label])=>{
        const active=href==="/"?pathname===href:href==="/payments"?pathname.startsWith("/payments"):pathname.startsWith(href);
        return <a key={href} href={href} className={active?"active":""}>{label}</a>;
      })}
    </nav>
    <Dialog.Trigger className="mobile-menu-button" aria-label="Open navigation" aria-controls="primary-navigation"><MenuIcon/><span>Menu</span></Dialog.Trigger>
    <Dialog.Portal>
      <Dialog.Backdrop className="navigation-backdrop"/>
      <Dialog.Popup className="mobile-navigation" id="primary-navigation" aria-label="Primary navigation">
        <div className="mobile-navigation-head"><span className="mobile-navigation-kicker">Workspace</span><Dialog.Close className="mobile-menu-close" aria-label="Close navigation"><XIcon/></Dialog.Close></div>
        <nav>{items.map(([href,label],index)=>{const active=href==="/"?pathname===href:href==="/payments"?pathname.startsWith("/payments"):pathname.startsWith(href);return <a key={href} href={href} className={active?"active":""} onClick={()=>setOpen(false)}><i>{String(index+1).padStart(2,"0")}</i><span>{label}</span><b>→</b></a>})}</nav>
        <p>Payments, transfers and settings in one focused workspace.</p>
      </Dialog.Popup>
    </Dialog.Portal>
  </Dialog.Root>;
}

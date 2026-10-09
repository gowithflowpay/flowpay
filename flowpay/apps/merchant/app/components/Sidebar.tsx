"use client";
import {Dialog} from "@base-ui/react/dialog";
import {useEffect,useState} from "react";
import {usePathname} from "next/navigation";
import {MenuIcon,XIcon,HomeIcon,PaymentIcon,ClockIcon,ClaimIcon,KeyIcon,SettingsIcon,PlusIcon,ArrowRightIcon,LifebuoyIcon} from "./Icons";
const items=[["/dashboard","Dashboard"],["/payments","Payments"],["/history","History"],["/claims","Claims"],["/developers","API Keys"],["/settings","Settings"]] as const;
const navigationIcons=[HomeIcon,PaymentIcon,ClockIcon,ClaimIcon,KeyIcon,SettingsIcon];
const descriptions=["Your business at a glance","Manage your payment links","Every payment, in one place","Review payment issues","Connect your application","Wallet, security and webhooks"];
export function Sidebar({merchantName}:{merchantName:string|null}){
  const pathname=usePathname();
  const [open,setOpen]=useState(false);
  const [signingOut,setSigningOut]=useState(false);
  useEffect(()=>setOpen(false),[pathname]);
  useEffect(()=>{
    const close=()=>setOpen(false);
    window.addEventListener("resize",close);
    return()=>window.removeEventListener("resize",close);
  },[]);
  async function signOut(){
    if(signingOut)return;
    setSigningOut(true);
    try{await fetch("/api/auth/logout",{method:"POST"});}catch{}
    // A full navigation guarantees no cached server render survives the sign-out.
    window.location.assign("/login");
  }
  function isActive(href:string){
    return href==="/dashboard"?pathname==="/dashboard":pathname.startsWith(href);
  }
  return <Dialog.Root open={open} onOpenChange={setOpen}>
    <nav className="primary-nav" aria-label="Primary navigation">
      {items.map(([href,label])=>(
        <a key={href} href={href} className={isActive(href)?"active":""}>{label}</a>
      ))}
      <button type="button" className="nav-signout" onClick={signOut} disabled={signingOut}>
        {signingOut?"Signing out…":"Sign out"}
      </button>
    </nav>
    <Dialog.Trigger className="mobile-menu-button" aria-label="Open navigation" aria-controls="primary-navigation"><MenuIcon/><span>Menu</span></Dialog.Trigger>
    <Dialog.Portal>
      <Dialog.Backdrop className="navigation-backdrop"/>
      <Dialog.Popup className="mobile-navigation workspace-navigation" id="primary-navigation" aria-describedby={undefined}>
        <div className="navigation-brand"><Dialog.Title><img src="/assets/flowpay-mark.svg" alt=""/>FlowPay</Dialog.Title><Dialog.Close className="navigation-close" aria-label="Close navigation"><XIcon/></Dialog.Close></div>
        <a href="/settings" className="navigation-account" onClick={()=>setOpen(false)}><span className="navigation-avatar">{(merchantName??"FlowPay").trim().split(/\s+/).slice(0,2).map(word=>word[0]).join("").toUpperCase()||"FP"}</span><span><strong>{merchantName??"Your business"}</strong><small>Merchant workspace</small></span><ArrowRightIcon/></a>
        <a className="navigation-create" href="/payments/new" onClick={()=>setOpen(false)}><PlusIcon/>Create payment link</a>
        <span className="navigation-section">WORKSPACE</span>
        <nav aria-label="Mobile primary navigation">{items.map(([href,label],index)=>{const Icon=navigationIcons[index];const active=isActive(href);return <a key={href} href={href} className={active?"active":""} aria-current={active?"page":undefined} onClick={()=>setOpen(false)}><span className="navigation-icon"><Icon/></span><span className="navigation-copy"><strong>{label}</strong><small>{descriptions[index]}</small></span><span className="navigation-indicator" aria-hidden="true"/></a>})}</nav>
        <div className="navigation-footer"><a href="/docs"><LifebuoyIcon/><span>Help & documentation</span><ArrowRightIcon/></a><button type="button" onClick={signOut} disabled={signingOut}><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" aria-hidden="true"><path d="M9 4H4v16h5M14 8l4 4-4 4M8 12h10"/></svg>{signingOut?"Signing out…":"Sign out"}</button><small>FlowPay · Payments made simple</small></div>
      </Dialog.Popup>
    </Dialog.Portal>
  </Dialog.Root>;
}

"use client";
import {Dialog} from "@base-ui/react/dialog";
import {useEffect,useState} from "react";
import {usePathname,useRouter} from "next/navigation";
import {MenuIcon,XIcon} from "./Icons";
const items=[["/dashboard","Dashboard"],["/payments","Payments"],["/history","History"],["/claims","Claims"],["/developers","API Keys"],["/settings","Settings"]] as const;
export function Sidebar(){
  const pathname=usePathname();
  const router=useRouter();
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
      <Dialog.Popup className="mobile-navigation" id="primary-navigation" aria-label="Primary navigation">
        <div className="mobile-navigation-head"><span className="mobile-navigation-kicker">Workspace</span><Dialog.Close className="mobile-menu-close" aria-label="Close navigation"><XIcon/></Dialog.Close></div>
        <nav>{items.map(([href,label],index)=>{return <a key={href} href={href} className={isActive(href)?"active":""} onClick={()=>setOpen(false)}><i>{String(index+1).padStart(2,"0")}</i><span>{label}</span><b>→</b></a>})}</nav>
        <button type="button" className="mobile-signout" onClick={signOut} disabled={signingOut}>{signingOut?"Signing out…":"Sign out"}</button>
        <p>Payments, transfers and settings in one focused workspace.</p>
      </Dialog.Popup>
    </Dialog.Portal>
  </Dialog.Root>;
}

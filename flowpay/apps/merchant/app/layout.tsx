import "./globals.scss";
import type {ReactNode} from "react";
import {BellIcon,ChevronDownIcon,LifebuoyIcon} from "./components/Icons";
import {Sidebar} from "./components/Sidebar";

export default function Layout({children}:{children:ReactNode}){
  return <html lang="en" suppressHydrationWarning><body><div className="app-shell"><Sidebar/><div className="workspace"><header className="topbar"><div className="top-actions"><button className="notification-button" aria-label="Notifications"><BellIcon/><i/></button><div className="top-divider"/><div className="top-account"><div className="top-avatar">UT</div><span><strong>Urban Tech</strong><small>Merchant account</small></span><ChevronDownIcon/></div></div></header><main className="main">{children}</main><a className="claim-fab" href="/claims" aria-label="Create claim"><LifebuoyIcon/><span>Create claim</span></a></div></div></body></html>;
}

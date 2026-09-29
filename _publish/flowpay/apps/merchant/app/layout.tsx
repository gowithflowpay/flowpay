import "./globals.scss";
import type {ReactNode} from "react";
import {ChevronDownIcon} from "./components/Icons";
import {Sidebar} from "./components/Sidebar";
import {ThemeToggle} from "./components/ThemeToggle";

export default function Layout({children}:{children:ReactNode}){
  return <html lang="en"><body><div className="app-shell"><header className="topbar">
    <a className="logo" href="/"><span>FlowPay</span></a>
    <Sidebar/>
    <div className="topbar-actions"><ThemeToggle/><button className="store-switcher" type="button"><span>Acme Store</span><ChevronDownIcon/></button></div>
  </header><main className="main">{children}</main></div></body></html>;
}

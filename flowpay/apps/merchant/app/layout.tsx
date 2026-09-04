import "./globals.scss";
import type {ReactNode} from "react";
import {ChevronDownIcon,StoreIcon} from "./components/Icons";
import {Sidebar} from "./components/Sidebar";

export default function Layout({children}:{children:ReactNode}){
  return <html lang="en"><body><div className="app-shell"><header className="topbar">
    <a className="logo" href="/"><span>FlowPay</span></a>
    <Sidebar/>
    <button className="store-switcher" type="button"><StoreIcon/><span>Acme Store</span><ChevronDownIcon/></button>
  </header><main className="main">{children}</main></div></body></html>;
}

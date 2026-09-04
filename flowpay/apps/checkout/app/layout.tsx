import type {Metadata} from "next";
import type {ReactNode} from "react";
import "./style.scss";
import "@fontsource/inter/400.css";
import "@fontsource/inter/500.css";
import "@fontsource/inter/600.css";
import "@fontsource/inter/700.css";
export const metadata:Metadata={title:"FlowPay Checkout",description:"Secure crypto checkout and recovery by FlowPay"};
export default function Layout({children}:{children:ReactNode}){return <html lang="en"><body>{children}</body></html>}

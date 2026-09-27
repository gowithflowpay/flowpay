import "./globals.scss";
import "./auth.scss";
import type {ReactNode} from "react";
import {AppShell} from "./components/AppShell";
import {getMerchant} from "../lib/session";

export const metadata={title:"FlowPay",description:"Accept payments and settle to your wallet."};

export default async function Layout({children}:{children:ReactNode}){
  // Signed-out visitors resolve to null rather than throwing, so auth pages
  // still render when the API is unreachable.
  const merchant=await getMerchant();
  return <html lang="en"><body>
    <AppShell merchantName={merchant?.business_name??null} contactName={merchant?.contact_name??null}>{children}</AppShell>
  </body></html>;
}

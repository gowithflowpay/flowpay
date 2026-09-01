"use client";
import {usePathname} from "next/navigation";
import {ClaimIcon,HomeIcon,KeyIcon,LinkIcon,PaymentIcon,SettingsIcon,WebhookIcon} from "./Icons";
const items=[
  ["/","Overview",HomeIcon],
  ["/payments","Payments",PaymentIcon],
  ["/payments/new","Payment Links",LinkIcon],
  ["/claims","Claims",ClaimIcon],
  ["/webhooks","Webhooks",WebhookIcon],
  ["/developers","API Keys",KeyIcon],
  ["/settings","Settings",SettingsIcon],
] as const;
export function Sidebar(){const pathname=usePathname();return <aside className="sidebar">
  <a className="logo" href="/"><img src="/assets/flowpay-mark.svg" alt=""/><span>FlowPay</span></a>
  <nav>{items.map(([href,label,Icon])=>{const active=href==="/"?pathname===href:href==="/payments"?pathname===href||pathname.startsWith("/payments/")&&pathname!=="/payments/new":pathname.startsWith(href);return <a key={href} href={href} className={active?"active":""}><Icon/><span>{label}</span></a>})}</nav>
  <div className="sidebar-bottom"><div className="merchant-mini"><div className="avatar">UT</div><div><strong>Urban Tech</strong><span>Merchant</span></div><b>⌄</b></div></div>
</aside>}

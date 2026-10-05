type ProductKind="checkout"|"webhooks"|"recovery"|"cli"|"mcp";
const flows={
 checkout:{caption:"ONE INTEGRATION. EVERY PAYMENT CONNECTED.",inputs:["BASE","ETHEREUM","POLYGON","ARBITRUM"],values:["115 USDC","120 USDC","80 USDC","900 USDC"],status:"PAID"},
 webhooks:{caption:"SIGNED EVENTS. DELIVERED TO YOUR PLATFORM.",inputs:["CREATED","DETECTED","CONFIRMED","SETTLED"],values:["payment","deposit","receipt","settlement"],status:"DELIVERED"},
 recovery:{caption:"VERIFIED EVIDENCE. GUIDED PAYMENT RECOVERY.",inputs:["TRANSFER","ASSET","NETWORK","CLAIM"],values:["Transaction","Token","Chain","Evidence"],status:"VERIFIED"},
 cli:{caption:"YOUR TERMINAL. CONNECTED TO PAYMENT RAILS.",inputs:["REGISTER","EMAIL","REQUEST","RECEIPT"],values:["Account","OTP code","20 USDC","Sepolia"],status:"READY"},
 mcp:{caption:"YOUR AI CLIENT. AUTHORIZED PAYMENT TOOLS.",inputs:["CHATGPT","CLAUDE","PKCE","SCOPES"],values:["Connect","Approve","Verified","Payments"],status:"CONNECTED"},
};
export function ProductFlow({kind}:{kind:ProductKind}){
 const flow=flows[kind];
 const routes=["M220 105V162Q220 190 250 190H260Q285 190 285 220V245H365","M120 210H252Q285 210 285 238V245H365","M120 310H252Q285 310 285 278V245H365","M220 415V355Q220 325 250 325H260Q285 325 285 295V245H365","M365 245H445V155Q445 105 480 105H575","M365 245H445V225Q445 210 480 210H615","M365 245H445V280Q445 310 480 310H615","M365 245H445V360Q445 415 480 415H575"];
 return <div className="product-flow" aria-hidden="true"><svg viewBox="0 0 730 510" role="presentation">
  <defs><pattern id={`flow-grid-${kind}`} width="50" height="50" patternUnits="userSpaceOnUse"><path d="M50 0H0V50" fill="none" stroke="white" strokeOpacity=".4"/></pattern><radialGradient id={`flow-glow-${kind}`}><stop stopColor="white"/><stop offset=".6" stopColor="#e6f3ff" stopOpacity=".65"/><stop offset="1" stopColor="#2563eb" stopOpacity=".5"/></radialGradient></defs>
  <rect x="60" y="45" width="610" height="420" fill={`url(#flow-grid-${kind})`} stroke="white" strokeOpacity=".4"/>
  <g fill="none" stroke="#537e9d" strokeWidth="1">{routes.map((d,i)=><path key={i} id={`flow-${kind}-${i}`} d={d}/>)}</g>
  {routes.map((d,i)=><path key={i} className="flow-particle" d="M-4-3L4 0-4 3Z" fill="#24588a"><animateMotion dur="4s" begin={`${-i*.6}s`} repeatCount="indefinite" rotate="auto"><mpath href={`#flow-${kind}-${i}`}/></animateMotion></path>)}
  <path d="M365 151V245" stroke="#2563eb" strokeDasharray="4 3"/>
  <rect x="273" y="115" width="184" height="38" rx="2" fill="white"/><text x="365" y="131" textAnchor="middle" className="flow-caption">{flow.caption.split('. ')[0]}.</text><text x="365" y="145" textAnchor="middle" className="flow-caption">{flow.caption.split('. ')[1]}</text>
  <circle className="flow-core" cx="365" cy="245" r="32" fill={`url(#flow-glow-${kind})`} stroke="#5088cf" strokeWidth="3"/><circle cx="365" cy="245" r="7" fill="#2563eb"/>
  {flow.inputs.map((label,i)=>{const x=i===0||i===3?174:74;const y=65+i*100;const right=i===0||i===3?540:580;return <g key={label}><rect x={x} y={y} width="84" height="80" rx="3" fill="white"/><rect x={x+8} y={y+8} width="68" height="29" rx="1" fill="#edf3f7"/><text x={x+14} y={y+26} className="flow-label">{label}</text><text x={x+9} y={y+65} className="flow-value">{flow.values[i]}</text><rect x={right} y={y} width="84" height="80" rx="3" fill="white"/><rect x={right+7} y={y+8} width="70" height="27" rx="2" fill="#24588a"/><text x={right+42} y={y+26} textAnchor="middle" className="flow-status">{flow.status}</text><rect x={right+7} y={y+44} width="70" height="28" fill="#edf3f7"/><text x={right+12} y={y+63} className="flow-result">{flow.values[i]}</text></g>})}
 </svg></div>;
}

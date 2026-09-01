export function statusTone(status:string){
  if(["COMPLETED","RECOVERED","CONFIRMED"].includes(status))return "success";
  if(["PARTIALLY_PAID","WAITING","DETECTED","CONFIRMING","RECOVERY_AVAILABLE","APPROVAL_PENDING","RECOVERY_PENDING","INVESTIGATING"].includes(status))return "warn";
  if(["FAILED","ESCALATED","REJECTED","NOT_RECOVERABLE","EXPIRED","CANCELLED"].includes(status))return "bad";
  if(["CLAIM_PENDING","WRONG_ASSET","WRONG_CHAIN_CLAIMED","NEEDS_MORE_EVIDENCE"].includes(status))return "info";
  return "neutral";
}
export function short(value:string|undefined|null,start=7,end=5){if(!value)return "—";return value.length>start+end+2?`${value.slice(0,start)}…${value.slice(-end)}`:value;}
export function moneyFromStableBalances(balances:any[]){
  let cents=0n;
  for(const b of balances??[]){
    if(!["USDC","USDT"].includes(String(b.symbol??"").toUpperCase())||!b.amount_atomic)continue;
    const decimals=BigInt(Number(b.decimals??6));
    const scale=10n**decimals;
    cents+=(BigInt(String(b.amount_atomic))*100n)/scale;
  }
  const dollars=cents/100n,frac=(cents%100n).toString().padStart(2,"0");
  return `$${dollars.toString().replace(/\B(?=(\d{3})+(?!\d))/g,",")}.${frac}`;
}
function chainKey(chain:unknown){
  if(typeof chain==="string")return chain.toLowerCase().replace(/^custom:/,"");
  if(chain&&typeof chain==="object"&&"custom" in chain)return String((chain as {custom:unknown}).custom).toLowerCase();
  return "base";
}
export function networkAsset(chain:unknown){
  const key=chainKey(chain);
  if(key==="bsc"||key==="bsc_testnet")return "/assets/bsc.svg";
  if(key==="ethereum_sepolia")return "/assets/ethereum.svg";
  if(key==="arbitrum_sepolia")return "/assets/arbitrum.svg";
  return "/assets/base.svg";
}
export function networkLabel(chain:unknown){
  const key=chainKey(chain);
  return ({base:"Base",bsc:"BNB Smart Chain",base_sepolia:"Base Sepolia",ethereum_sepolia:"Ethereum Sepolia",arbitrum_sepolia:"Arbitrum Sepolia",bsc_testnet:"BSC Testnet"} as Record<string,string>)[key]??key;
}
export function tokenAsset(symbol:string){return symbol.toUpperCase()==="USDT"?"/assets/usdt.svg":"/assets/usdc.svg";}

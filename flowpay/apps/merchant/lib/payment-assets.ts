import {networkAsset,networkLabel} from "./format";

export type PaymentAsset={symbol:string;chain:string;decimals:number;contract:string|null};
export type PaymentOption={value:string;label:string;detail:string;icon:string};
const tokenMeta:Record<string,{name:string;icon:string}>={
  USDC:{name:"USD Coin",icon:"/assets/usdc.svg"},
  USDT:{name:"Tether",icon:"/assets/usdt.svg"},
  ETH:{name:"Ether",icon:"/assets/ethereum.svg"},
  BNB:{name:"BNB",icon:"/assets/bsc.svg"},
};
export function assetOptions(catalog:PaymentAsset[]):PaymentOption[]{
  return [...new Set(catalog.map(asset=>asset.symbol))].sort((a,b)=>a==="USDC"?-1:b==="USDC"?1:a.localeCompare(b)).map(symbol=>({value:symbol,label:symbol,detail:tokenMeta[symbol]?.name??symbol,icon:tokenMeta[symbol]?.icon??"/assets/flowpay-mark.svg"}));
}
export function networkOptions(catalog:PaymentAsset[],symbol:string):PaymentOption[]{
  return [...new Set(catalog.filter(asset=>asset.symbol===symbol).map(asset=>asset.chain))].map(chain=>({value:chain,label:networkLabel(chain),detail:chain.replace(/^custom:/,"").includes("testnet")||chain.includes("sepolia")||chain.includes("amoy")?"Test network":"Mainnet",icon:networkAsset(chain)}));
}

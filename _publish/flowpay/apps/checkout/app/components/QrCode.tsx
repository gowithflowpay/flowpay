"use client";

import {useMemo} from "react";

// Small self-contained QR encoder for hosted checkout addresses.
// It intentionally uses QR version 4-L (33x33), byte mode, one RS block.
// That is sufficient for EVM addresses and keeps checkout reproduction offline.
const SIZE=33, DATA_CODEWORDS=80, ECC_CODEWORDS=20;

function gfTables(){
  const exp=new Array<number>(512).fill(0),log=new Array<number>(256).fill(0);let x=1;
  for(let i=0;i<255;i++){exp[i]=x;log[x]=i;x<<=1;if(x&0x100)x^=0x11d;}
  for(let i=255;i<512;i++)exp[i]=exp[i-255];return {exp,log};
}
const GF=gfTables();
function gfMul(a:number,b:number){if(a===0||b===0)return 0;return GF.exp[GF.log[a]+GF.log[b]];}
function multiply(a:number[],b:number[]){const out=new Array(a.length+b.length-1).fill(0);for(let i=0;i<a.length;i++)for(let j=0;j<b.length;j++)out[i+j]^=gfMul(a[i],b[j]);return out;}
function ecc(data:number[]){let gen=[1];for(let i=0;i<ECC_CODEWORDS;i++)gen=multiply(gen,[1,GF.exp[i]]);const msg=[...data,...new Array(ECC_CODEWORDS).fill(0)];for(let i=0;i<data.length;i++){const factor=msg[i];if(!factor)continue;for(let j=0;j<gen.length;j++)msg[i+j]^=gfMul(gen[j],factor);}return msg.slice(data.length);}
function bitsPush(bits:number[],value:number,length:number){for(let i=length-1;i>=0;i--)bits.push((value>>>i)&1);}
function makeCodewords(text:string){
  const bytes=[...new TextEncoder().encode(text)];if(bytes.length>78)throw new Error("QR payload too long");const bits:number[]=[];
  bitsPush(bits,0b0100,4);bitsPush(bits,bytes.length,8);for(const b of bytes)bitsPush(bits,b,8);
  const cap=DATA_CODEWORDS*8;for(let i=0;i<4&&bits.length<cap;i++)bits.push(0);while(bits.length%8)bits.push(0);
  const data:number[]=[];for(let i=0;i<bits.length;i+=8){let b=0;for(let j=0;j<8;j++)b=(b<<1)|(bits[i+j]??0);data.push(b);}
  let flip=false;while(data.length<DATA_CODEWORDS){data.push(flip?0x11:0xec);flip=!flip;}return [...data,...ecc(data)];
}
function formatBits(mask:number){const ecl=1;let data=(ecl<<3)|mask;let d=data<<10;const gen=0x537;for(let i=14;i>=10;i--)if((d>>>i)&1)d^=gen<<(i-10);return (((data<<10)|d)^0x5412)&0x7fff;}
function buildQr(text:string){
  const m:Array<Array<boolean|null>>=Array.from({length:SIZE},()=>Array<boolean|null>(SIZE).fill(null));
  const set=(r:number,c:number,v:boolean)=>{if(r>=0&&r<SIZE&&c>=0&&c<SIZE)m[r][c]=v;};
  const finder=(row:number,col:number)=>{for(let r=-1;r<=7;r++)for(let c=-1;c<=7;c++){if(row+r<0||row+r>=SIZE||col+c<0||col+c>=SIZE)continue;const on=(r>=0&&r<=6&&(c===0||c===6))||(c>=0&&c<=6&&(r===0||r===6))||(r>=2&&r<=4&&c>=2&&c<=4);set(row+r,col+c,on);}};
  finder(0,0);finder(SIZE-7,0);finder(0,SIZE-7);
  for(let i=8;i<SIZE-8;i++){if(m[i][6]===null)set(i,6,i%2===0);if(m[6][i]===null)set(6,i,i%2===0);}
  const align=(row:number,col:number)=>{for(let r=-2;r<=2;r++)for(let c=-2;c<=2;c++)set(row+r,col+c,Math.max(Math.abs(r),Math.abs(c))!==1);};align(26,26);
  const fb=formatBits(0);for(let i=0;i<15;i++){const on=((fb>>>i)&1)!==0;
    if(i<6)set(i,8,on);else if(i<8)set(i+1,8,on);else set(SIZE-15+i,8,on);
    if(i<8)set(8,SIZE-i-1,on);else if(i<9)set(8,15-i,on);else set(8,14-i,on);
  }set(SIZE-8,8,true);
  const codewords=makeCodewords(text);const dataBits:number[]=[];for(const b of codewords)bitsPush(dataBits,b,8);let bit=0,row=SIZE-1,dir=-1;
  for(let col=SIZE-1;col>0;col-=2){if(col===6)col--;for(;;){for(let j=0;j<2;j++){const c=col-j;if(m[row][c]!==null)continue;let v=(dataBits[bit++]??0)===1;if((row+c)%2===0)v=!v;set(row,c,v);}row+=dir;if(row<0||row>=SIZE){row-=dir;dir=-dir;break;}}}
  return m as boolean[][];
}

export function QrCode({value,label="Payment address"}:{value:string;label?:string}){
  const matrix=useMemo(()=>buildQr(value),[value]);const quiet=4,view=SIZE+quiet*2;
  return <svg className="qr-svg" viewBox={`0 0 ${view} ${view}`} role="img" aria-label={`${label} QR code`} shapeRendering="crispEdges">
    <rect width={view} height={view} rx="2" fill="white"/>
    {matrix.flatMap((row,r)=>row.map((on,c)=>on?<rect key={`${r}-${c}`} x={c+quiet} y={r+quiet} width="1" height="1" fill="#090B13"/>:null))}
  </svg>
}

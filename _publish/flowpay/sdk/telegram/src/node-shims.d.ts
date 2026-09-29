declare const process:{env:Record<string,string|undefined>};
declare module "node:http" {
  export type IncomingMessage = any; export type ServerResponse = any;
  export function createServer(handler:(req:IncomingMessage,res:ServerResponse)=>void|Promise<void>):{listen:(port:number,host:string,cb:()=>void)=>void};
}

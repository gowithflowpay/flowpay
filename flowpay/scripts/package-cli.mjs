import {cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, realpathSync, rmSync, writeFileSync} from "node:fs";
import {tmpdir} from "node:os";
import path from "node:path";
import {fileURLToPath} from "node:url";
import {execFileSync} from "node:child_process";

const project=path.resolve(path.dirname(fileURLToPath(import.meta.url)),"..");
const output=path.resolve(process.argv[2]??path.join(project,"../.local/releases"));
const cli=path.join(project,"apps/cli");
const sdk=path.join(project,"sdk/node");
for(const file of [path.join(cli,"dist/index.js"),path.join(sdk,"dist/index.js")]){
  if(!existsSync(file))throw new Error("Build sdk/node and apps/cli before packaging");
}
mkdirSync(output,{recursive:true});
const temporary=mkdtempSync(path.join(tmpdir(),"flowpay-cli-"));
const stage=path.join(temporary,"package");
try{
  mkdirSync(stage);
  cpSync(path.join(cli,"dist"),path.join(stage,"dist"),{recursive:true});
  cpSync(path.join(cli,"README.md"),path.join(stage,"README.md"));
  const manifest=JSON.parse(readFileSync(path.join(cli,"package.json"),"utf8"));
  const sdkManifest=JSON.parse(readFileSync(path.join(sdk,"package.json"),"utf8"));
  manifest.dependencies["@flowpay/node"]=sdkManifest.version;
  delete manifest.devDependencies;
  writeFileSync(path.join(stage,"package.json"),JSON.stringify(manifest,null,2)+"\n");
  const bundled=path.join(stage,"node_modules/@flowpay/node");
  mkdirSync(bundled,{recursive:true});
  cpSync(path.join(sdk,"package.json"),path.join(bundled,"package.json"));
  cpSync(path.join(sdk,"dist"),path.join(bundled,"dist"),{recursive:true});
  const npmCli=process.env.npm_execpath;
  if(npmCli)execFileSync(process.execPath,[npmCli,"pack","--ignore-scripts","--pack-destination",output],{cwd:stage,stdio:"inherit"});
  else if(process.platform!=="win32")execFileSync("npm",["pack","--ignore-scripts","--pack-destination",output],{cwd:stage,stdio:"inherit"});
  else throw new Error("On Windows, run this script through npm exec -- node scripts/package-cli.mjs");
}finally{
  const resolved=realpathSync(temporary);
  if(path.dirname(resolved)!==realpathSync(tmpdir())||!path.basename(resolved).startsWith("flowpay-cli-"))throw new Error("Unexpected package staging directory");
  rmSync(resolved,{recursive:true,force:true});
}

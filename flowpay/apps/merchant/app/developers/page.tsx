import {api} from "../../lib/api";
import {ApiKeysPanel} from "./ApiKeysPanel";

export default async function ApiKeysPage(){
  const result=await api("/v1/api-keys");
  return <ApiKeysPanel initialKeys={result.data??[]}/>;
}

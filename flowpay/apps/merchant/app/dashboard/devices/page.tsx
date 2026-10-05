import {requireMerchant} from "../../../lib/session";
import {DeviceApprovalPanel} from "./DeviceApprovalPanel";

export default async function DevicesPage(){
  await requireMerchant();
  return <DeviceApprovalPanel/>;
}

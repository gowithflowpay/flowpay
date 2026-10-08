"use client";
import {useTransition} from "react";
import {useRouter} from "next/navigation";
export function RefreshData(){const router=useRouter();const [pending,start]=useTransition();return <button className="workspace-secondary" disabled={pending} onClick={()=>start(()=>router.refresh())}>{pending?"Refreshing...":"Try again"}</button>;}

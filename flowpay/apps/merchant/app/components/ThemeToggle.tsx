"use client";
import {useEffect,useState} from "react";
import {MoonIcon,SunIcon} from "./Icons";

export function ThemeToggle(){
  const [dark,setDark]=useState(false);
  useEffect(()=>{const saved=localStorage.getItem("flowpay-theme");const next=saved?saved==="dark":matchMedia("(prefers-color-scheme: dark)").matches;setDark(next);document.documentElement.dataset.theme=next?"dark":"light"},[]);
  function toggle(){const next=!dark;setDark(next);document.documentElement.dataset.theme=next?"dark":"light";localStorage.setItem("flowpay-theme",next?"dark":"light")}
  return <button className="theme-button" aria-label={dark?"Use light mode":"Use dark mode"} title={dark?"Use light mode":"Use dark mode"} onClick={toggle}>{dark?<SunIcon/>:<MoonIcon/>}</button>;
}

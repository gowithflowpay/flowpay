"use client";

import Image from "next/image";
import {useEffect,useRef,useState} from "react";
import {CheckIcon} from "../../components/Icons";

type Option={value:string;label:string;detail:string;icon:string};

export function SelectField({name,label,options,onValueChange}:{name:string;label:string;options:Option[];onValueChange?:(option:Option)=>void}){
  const [value,setValue]=useState(options[0].value);
  const [open,setOpen]=useState(false);
  const root=useRef<HTMLDivElement>(null);
  const selected=options.find(option=>option.value===value)??options[0];

  useEffect(()=>{
    const close=(event:MouseEvent)=>{if(!root.current?.contains(event.target as Node))setOpen(false)};
    document.addEventListener("mousedown",close);
    return()=>document.removeEventListener("mousedown",close);
  },[]);

  return <div className="field custom-select-field" ref={root}>
    <span>{label}</span>
    <input type="hidden" name={name} value={value}/>
    <button className={`select-trigger${open?" open":""}`} type="button" aria-haspopup="listbox" aria-expanded={open} onClick={()=>setOpen(current=>!current)}>
      <Image src={selected.icon} width={28} height={28} alt=""/>
      <span className="select-copy"><strong>{selected.label}</strong><small>{selected.detail}</small></span>
      <svg className="select-chevron" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" aria-hidden="true"><path d="m7 10 5 5 5-5"/></svg>
    </button>
    {open?<div className="select-menu" role="listbox" aria-label={label}>
      {options.map(option=><button type="button" role="option" aria-selected={option.value===value} className="select-option" key={option.value} onClick={()=>{setValue(option.value);setOpen(false);onValueChange?.(option)}}>
        <Image src={option.icon} width={30} height={30} alt=""/>
        <span><strong>{option.label}</strong><small>{option.detail}</small></span>
        {option.value===value?<CheckIcon/>:null}
      </button>)}
    </div>:null}
  </div>;
}

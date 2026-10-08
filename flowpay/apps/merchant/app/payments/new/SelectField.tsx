"use client";

import {Field} from "@base-ui/react/field";
import {Select} from "@base-ui/react/select";
import Image from "next/image";
import {useState} from "react";
import {CheckIcon} from "../../components/Icons";

type Option={value:string;label:string;detail:string;icon:string};

export function SelectField({name,label,options,onValueChange}:{name:string;label:string;options:Option[];onValueChange?:(option:Option)=>void}){
  const [value,setValue]=useState(options[0].value);
  const selected=options.find(option=>option.value===value)??options[0];

  return <Field.Root className="field custom-select-field" name={name}>
    <Select.Root value={value} onValueChange={(nextValue)=>{const next=options.find(option=>option.value===nextValue);setValue(String(nextValue));if(next)onValueChange?.(next)}}>
      <Select.Label>{label}</Select.Label>
      <Select.Trigger className="select-trigger">
        <Image src={selected.icon} width={28} height={28} alt=""/>
        <Select.Value className="select-copy"><strong>{selected.label}</strong><small>{selected.detail}</small></Select.Value>
        <Select.Icon className="select-chevron">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" aria-hidden="true"><path d="m7 10 5 5 5-5"/></svg>
        </Select.Icon>
      </Select.Trigger>
      <Select.Portal>
        <Select.Positioner className="select-positioner" alignItemWithTrigger={false}>
          <Select.Popup className="select-menu">
            <Select.List>
              {options.map(option=><Select.Item value={option.value} className="select-option" key={option.value}>
                <Image src={option.icon} width={30} height={30} alt=""/>
                <Select.ItemText className="select-copy"><strong>{option.label}</strong><small>{option.detail}</small></Select.ItemText>
                <Select.ItemIndicator><CheckIcon/></Select.ItemIndicator>
              </Select.Item>)}
            </Select.List>
          </Select.Popup>
        </Select.Positioner>
      </Select.Portal>
    </Select.Root>
  </Field.Root>;
}

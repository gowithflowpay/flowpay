"use client";
import {useEffect,useRef} from "react";
import type {AnimationItem} from "lottie-web";

export function BenefitArt({index}:{index:number}){
  const container=useRef<HTMLDivElement>(null);
  useEffect(()=>{
    const node=container.current;
    if(!node)return;
    let disposed=false;
    let animation:AnimationItem|undefined;
    const motion=window.matchMedia("(prefers-reduced-motion: reduce)");
    const observer=new IntersectionObserver(async entries=>{
      if(!entries[0].isIntersecting){animation?.pause();return;}
      if(animation){if(!motion.matches)animation.play();return;}
      observer.unobserve(node);
      try{
        const [{default:lottie},response]=await Promise.all([import("lottie-web"),fetch(`/assets/benefits/benefit-${index}.json`)]);
        if(!response.ok)return;
        const animationData=await response.json();
        if(disposed)return;
        animation=lottie.loadAnimation({container:node,renderer:"svg",loop:true,autoplay:!motion.matches,animationData});
        animation.addEventListener("DOMLoaded",()=>{node.classList.add("benefit-art-loaded");if(motion.matches)animation?.goToAndStop(0,true);});
        observer.observe(node);
      }catch{/* The static illustration remains visible if animation cannot load. */}
    },{rootMargin:"100px"});
    observer.observe(node);
    return()=>{disposed=true;observer.disconnect();animation?.destroy();};
  },[index]);
  return <div className="benefit-art" aria-hidden="true"><img src={`/assets/benefits/benefit-${index}.svg`} alt=""/><div ref={container} className="benefit-animation"/></div>;
}

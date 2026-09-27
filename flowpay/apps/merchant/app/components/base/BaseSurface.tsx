import type {HTMLAttributes,ReactNode,TableHTMLAttributes,TdHTMLAttributes,ThHTMLAttributes} from "react";

type KickerProps={children:ReactNode;className?:string};

function cx(...classes:Array<string|false|undefined>){
  return classes.filter(Boolean).join(" ");
}

export function BaseCard({className,children,...props}:HTMLAttributes<HTMLElement>){
  return <section className={cx("base-card",className)} {...props}>{children}</section>;
}

export function BaseHeading({className,children,...props}:HTMLAttributes<HTMLHeadingElement>){
  return <h1 className={cx("base-heading",className)} {...props}>{children}</h1>;
}

export function BaseText({className,children,...props}:HTMLAttributes<HTMLParagraphElement>){
  return <p className={cx("base-text",className)} {...props}>{children}</p>;
}

export function BaseTable({className,children,...props}:TableHTMLAttributes<HTMLTableElement>){
  return <table className={cx("base-table table",className)} {...props}>{children}</table>;
}

export function BaseTh({className,children,...props}:ThHTMLAttributes<HTMLTableCellElement>){
  return <th className={cx("base-th",className)} {...props}>{children}</th>;
}

export function BaseTd({className,children,...props}:TdHTMLAttributes<HTMLTableCellElement>){
  return <td className={cx("base-td",className)} {...props}>{children}</td>;
}

export function BaseKicker({className,children}:KickerProps){
  return <div className={cx("base-kicker eyebrow",className)}>{children}</div>;
}

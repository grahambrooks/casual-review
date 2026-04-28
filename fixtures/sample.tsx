// TODO: extract to a proper component library
import * as React from "react";

export function Button({ label }: { label: string }) {
    return <button>{label}</button>;
}

// XXX: needs proper accessibility attributes
export function Card({ title }: { title: string }) {
    return <div className="card"><h2>{title}</h2></div>;
}

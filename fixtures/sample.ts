// TODO: replace any types with proper interfaces
export function add(a: number, b: number): number {
    return a + b;
}

/* FIXME: this should validate input */
export function parse(input: string): unknown {
    return JSON.parse(input);
}

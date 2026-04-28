// @ts-nocheck

export function f(x: { a?: string }) {
    // @ts-ignore: legacy code
    const a = (x as any).a!.toUpperCase();
    // @ts-expect-error
    const b: string = 42;
    return a + b;
}

export function untypedHandler(req: any): any {
    return req.body;
}

export function logsToConsole(x: number) {
    console.log("value", x);
    console.debug("d", x);
    console.warn("w", x);
    return x;
}

export function noisyAny() {
    const cache: Record<string, any> = {};
    return cache;
}

export function safe() {
    try {
        risky();
    } catch (e) {}
}

export function alsoSafe() {
    try {
        risky();
    } catch {}
}

declare function risky(): void;

export function suite() {
    it.skip("broken", () => {});
    xit("legacy", () => {});
    describe.skip("group", () => {});
    it.only("focused", () => {});
    it("enabled", () => {});
}

declare const it: any;
declare const xit: any;
declare const describe: any;

declare const it: any;
declare const test: any;
declare const describe: any;
declare const expect: any;

// Should NOT fire — has expect.
it("adds correctly", () => {
    expect(2 + 2).toBe(4);
});

// Should fire — empty body.
it("nothing here", () => {});

// Should fire — body but no assertion.
it("forgot expect", () => {
    const x = 2 + 2;
    console.log(x);
});

// Should NOT fire — test() function form.
test("subtraction", () => {
    expect(2 - 1).toBe(1);
});

// Nested in describe — both inner its should be classified independently.
describe("group", () => {
    it("works", () => {
        expect(true).toBe(true);
    });
    it("doesn't assert", () => {
        const noop = () => {};
        noop();
    });
});

// Score 0 — straight-line code, no branches.
fn linear() -> i32 {
    let a = 1;
    let b = 2;
    a + b
}

// Score 3 — three flat ifs at nesting 0. Each: +1 + 0.
fn flat_three(a: bool, b: bool, c: bool) -> i32 {
    if a {
        return 1;
    }
    if b {
        return 2;
    }
    if c {
        return 3;
    }
    0
}

// Score 6 — three nested ifs. Outer: +1+0=1. Middle: +1+1=2. Inner: +1+2=3.
fn nested_three(a: bool, b: bool, c: bool) -> i32 {
    if a {
        if b {
            if c {
                return 3;
            }
        }
    }
    0
}

// Should fire — score 21+ with five-level nesting and short-circuits.
fn very_complex(a: bool, b: bool, c: bool, d: bool, e: bool, f: bool) -> i32 {
    if a && b {
        for _i in 0..10 {
            if c || d {
                while e {
                    match f {
                        true => {
                            if a && c {
                                return 1;
                            }
                        }
                        false => {
                            if b && d {
                                return 2;
                            }
                        }
                    }
                }
            }
        }
    }
    0
}

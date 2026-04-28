fn small() -> i32 {
    42
}

fn uses_unwrap() -> i32 {
    let x: Result<i32, ()> = Ok(5);
    x.unwrap()
}

fn uses_expect() -> i32 {
    let x: Option<i32> = Some(7);
    x.expect("missing value")
}

fn debug_macros() {
    println!("hello");
    eprintln!("err");
    dbg!(42);
}

fn very_long_function() {
    let _a = 1;
    let _b = 2;
    let _c = 3;
    let _d = 4;
    let _e = 5;
    let _f = 6;
    let _g = 7;
    let _h = 8;
    let _i = 9;
    let _j = 10;
    let _k = 11;
    let _l = 12;
    let _m = 13;
    let _n = 14;
    let _o = 15;
    let _p = 16;
    let _q = 17;
    let _r = 18;
    let _s = 19;
    let _t = 20;
    let _u = 21;
    let _v = 22;
    let _w = 23;
    let _x = 24;
    let _y = 25;
    let _z = 26;
    let _aa = 27;
    let _bb = 28;
    let _cc = 29;
    let _dd = 30;
    let _ee = 31;
    let _ff = 32;
    let _gg = 33;
    let _hh = 34;
    let _ii = 35;
    let _jj = 36;
    let _kk = 37;
    let _ll = 38;
    let _mm = 39;
    let _nn = 40;
    let _oo = 41;
    let _pp = 42;
    let _qq = 43;
}

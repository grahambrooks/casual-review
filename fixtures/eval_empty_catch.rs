fn handle(r: Result<i32, ()>) {
    match r {
        Ok(_) => {
            let _ = 1;
        }
        Err(_) => {}
    }
}

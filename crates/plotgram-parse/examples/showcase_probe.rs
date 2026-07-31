fn main() {
    let mut ok=0; let mut fail=0;
    for path in std::env::args().skip(1) {
        let source = std::fs::read_to_string(&path).unwrap();
        match plotgram_parse::parse(&source) {
            Ok(_) => ok+=1,
            Err(e) => { eprintln!("FAIL {path}: {e}"); fail+=1; }
        }
    }
    eprintln!("showcase parse: {ok} ok, {fail} fail");
}

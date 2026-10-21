//! Applique `parse_file_name` à une liste de noms (un par ligne) — outil de diagnostic.
fn main() {
    let text = std::fs::read_to_string(std::env::args().nth(1).expect("fichier de noms")).unwrap();
    let (mut ok, mut ko) = (0, 0);
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        match comicrust_core::index::parse_file_name(line) {
            Some((s, n)) => { ok += 1; if ok <= 3 { println!("OK  {s} | {n} <- {line}"); } }
            None => { ko += 1; println!("KO  {line}"); }
        }
    }
    println!("{ok} reconnus, {ko} non reconnus");
}

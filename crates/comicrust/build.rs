//! Icône et informations de version de l'exécutable Windows.
fn main() {
    // Pile du fil principal : 8 Mio au lieu de 1 Mio sous Windows (MSVC), comme les autres
    // fils (voir `main.rs`) ; un débordement de pile ferme le programme sans message.
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        println!("cargo:rustc-link-arg-bins=/STACK:8388608");
    }
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        if let Err(e) = res.compile() {
            println!("cargo:warning=ressources Windows non compilées : {e}");
        }
    }
}

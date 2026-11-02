//! Script de build : génère le contexte Tauri (icône, manifeste Windows) — uniquement avec la
//! fonction `app`, pour ne rien exiger de Tauri dans le build par défaut du workspace.
fn main() {
    #[cfg(feature = "app")]
    {
        // Pile du fil principal : 8 Mio sous Windows (MSVC), comme la version egui.
        if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
            println!("cargo:rustc-link-arg-bins=/STACK:8388608");
        }
        tauri_build::build();
    }
}

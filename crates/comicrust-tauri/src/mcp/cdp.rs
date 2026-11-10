//! Appels au protocole DevTools de Chromium à travers WebView2
//! (`ICoreWebView2::CallDevToolsProtocolMethod`).
//!
//! Ce protocole donne des captures exactes (`Page.captureScreenshot`), des événements souris
//! et clavier « réels » (`Input.*`, traités comme ceux de l'utilisateur) et l'évaluation de
//! JavaScript avec retour de valeur (`Runtime.evaluate`). Windows uniquement.

use serde_json::Value;
use tauri::{AppHandle, Manager};

/// Délai maximal d'une réponse de la WebView.
const DELAI: std::time::Duration = std::time::Duration::from_secs(30);

/// Appelle une méthode DevTools sur la fenêtre principale et renvoie son résultat JSON.
#[cfg(windows)]
pub fn appeler(app: &AppHandle, methode: &str, params: Value) -> Result<Value, String> {
    use std::sync::mpsc;
    use webview2_com::CallDevToolsProtocolMethodCompletedHandler;
    use windows::core::HSTRING;

    let fenetre = app.get_webview_window("main").ok_or("fenêtre principale introuvable")?;
    let (tx, rx) = mpsc::channel::<Result<String, String>>();
    let methode_txt = methode.to_string();
    let params_txt = params.to_string();
    fenetre
        .with_webview(move |pw| {
            let tx_rappel = tx.clone();
            let lancement = (|| -> windows::core::Result<()> {
                // SAFETY : appels COM sur le fil de l'interface (garanti par with_webview), avec
                // des interfaces valides fournies par Tauri ; les chaînes vivent jusqu'au retour.
                unsafe {
                    let core = pw.controller().CoreWebView2()?;
                    let rappel = CallDevToolsProtocolMethodCompletedHandler::create(Box::new(
                        move |resultat: windows::core::Result<()>, json: String| {
                            let _ = tx_rappel.send(resultat.map(|_| json).map_err(|e| e.message()));
                            Ok(())
                        },
                    ));
                    core.CallDevToolsProtocolMethod(
                        &HSTRING::from(methode_txt.as_str()),
                        &HSTRING::from(params_txt.as_str()),
                        &rappel,
                    )
                }
            })();
            if let Err(e) = lancement {
                let _ = tx.send(Err(e.message()));
            }
        })
        .map_err(|e| e.to_string())?;
    let json = rx
        .recv_timeout(DELAI)
        .map_err(|_| "la fenêtre n'a pas répondu à temps (réduite ou bloquée ?)".to_string())?
        .map_err(|e| format!("{methode} : {e}"))?;
    serde_json::from_str(&json).map_err(|e| format!("{methode} : réponse illisible ({e})"))
}

/// Hors Windows (WebKitGTK), le protocole DevTools n'est pas disponible.
#[cfg(not(windows))]
pub fn appeler(app: &AppHandle, methode: &str, _params: Value) -> Result<Value, String> {
    let _ = app.get_webview_window("main");
    Err(format!("{methode} : pilotage disponible sous Windows uniquement (WebView2)"))
}

/// Évalue une expression JavaScript dans la page et renvoie sa valeur (promesses attendues).
pub fn evaluer(app: &AppHandle, expression: &str) -> Result<Value, String> {
    let r = appeler(
        app,
        "Runtime.evaluate",
        serde_json::json!({
            "expression": expression,
            "returnByValue": true,
            "awaitPromise": true,
            "userGesture": true,
        }),
    )?;
    if let Some(ex) = r.get("exceptionDetails") {
        let msg = ex
            .pointer("/exception/description")
            .or_else(|| ex.get("text"))
            .and_then(Value::as_str)
            .unwrap_or("erreur JavaScript");
        return Err(msg.lines().next().unwrap_or(msg).trim_start_matches("Error: ").to_string());
    }
    Ok(r.pointer("/result/value").cloned().unwrap_or(Value::Null))
}

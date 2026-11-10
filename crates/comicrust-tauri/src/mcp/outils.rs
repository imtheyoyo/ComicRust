//! Outils MCP de la version Tauri : capture, arbre des éléments, souris, clavier,
//! JavaScript, commandes du backend en lecture seule.
//!
//! Les coordonnées sont en pixels CSS (ceux de `getBoundingClientRect`), comme les clics du
//! protocole DevTools ; la capture est mise à la même échelle.

use std::time::{Duration, Instant};

use serde_json::{json, Value};
use tauri::AppHandle;

use super::cdp::{appeler, evaluer};
use super::protocol::{Reply, Tool};

/// Script injecté (journal, arbre, recherche d'éléments).
pub const SCRIPT: &str = include_str!("inject.js");

/// Commandes du backend appelables par `invoke` : lecture seule uniquement.
const COMMANDES_LECTURE: &[&str] = &[
    "infos",
    "lister_themes",
    "lire_preferences_vue",
    "lister_livres",
    "detail_livre",
    "lister_dossiers",
    "url_page_base",
    "lire_reglages_edition",
    "lire_fiches",
    "proposer_valeurs",
    "verifier_champs",
    "lire_pages",
    "modeles_de_nom",
    "etat_listes",
    "arbre_listes",
    "bilan_listes",
    "infos_suppression",
    "destinations_liste",
    "place_depot_liste",
    "infos_conversion",
    "depot_livres_possible",
    "groupes_vue",
    "catalogue_criteres",
    "nouveau_critere",
    "lire_criteres",
    "apercu_criteres",
    "analyser_cbl",
];

/// Appelle une fonction de `window.__mcp` (script injecté au besoin).
fn mcp_js(app: &AppHandle, appel: &str) -> Result<Value, String> {
    evaluer(app, &format!("{SCRIPT};\n{appel}"))
}

pub fn traiter(app: &AppHandle, nom: &str, args: &Value) -> Reply {
    let r = match nom {
        "screenshot" => return capture(app, args),
        "ui_tree" => arbre(app, args),
        "get_state" => mcp_js(app, "window.__mcp.etat()").map(Reply::Json),
        "click" => position(args, "x", "y").and_then(|(x, y)| cliquer(app, x, y, args)),
        "click_widget" => viser(app, args)
            .and_then(|(x, y, desc)| cliquer(app, x, y, args).map(|_| Reply::Text(format!("ok : clic sur {desc}")))),
        "hover" => position(args, "x", "y").and_then(|(x, y)| {
            souris(app, "mouseMoved", x, y, "none", 0, 0, modificateurs(args)?)?;
            let attente = args.get("hold_ms").and_then(Value::as_u64).unwrap_or(0).min(10_000);
            std::thread::sleep(Duration::from_millis(attente));
            Ok(Reply::Text(format!("ok : survol de ({x:.0}, {y:.0})")))
        }),
        "drag" => glisser(app, args),
        "scroll" => defiler(app, args),
        "type_text" => saisir(app, args),
        "press_key" => touche(app, args),
        "wait_for" => attendre(app, args),
        "wait" => {
            let ms = args.get("ms").and_then(Value::as_u64).unwrap_or(500).min(20_000);
            std::thread::sleep(Duration::from_millis(ms));
            Ok(Reply::Text(format!("attendu {ms} ms")))
        }
        "get_logs" => {
            let n = args.get("lines").and_then(Value::as_u64).unwrap_or(80).clamp(1, 1000);
            let filtre = args.get("filter").and_then(Value::as_str).unwrap_or("");
            mcp_js(app, &format!("window.__mcp.journal({n}, {})", json!(filtre))).map(|v| {
                let t = v.as_str().unwrap_or_default();
                Reply::Text(if t.is_empty() {
                    "journal de la page vide (console et erreurs JavaScript)".into()
                } else {
                    t.to_string()
                })
            })
        }
        "eval_js" => match args.get("expression").and_then(Value::as_str) {
            Some(e) => evaluer(app, e).map(Reply::Json),
            None => Err("argument « expression » manquant".into()),
        },
        "invoke" => invoquer(app, args),
        "reload_page" => evaluer(app, "location.reload(), true").map(|_| Reply::Text("page rechargée".into())),
        _ => Err(format!("outil inconnu : {nom}")),
    };
    r.unwrap_or_else(Reply::Error)
}

fn position(args: &Value, kx: &str, ky: &str) -> Result<(f64, f64), String> {
    match (args.get(kx).and_then(Value::as_f64), args.get(ky).and_then(Value::as_f64)) {
        (Some(x), Some(y)) => Ok((x, y)),
        _ => Err(format!("arguments « {kx} » et « {ky} » requis (pixels CSS)")),
    }
}

/// Masque des modificateurs du protocole DevTools : Alt 1, Ctrl 2, Meta 4, Maj 8.
fn modificateur(nom: &str) -> Option<u32> {
    match nom.to_lowercase().as_str() {
        "alt" => Some(1),
        "ctrl" | "control" | "ctl" => Some(2),
        "cmd" | "command" | "meta" | "win" => Some(4),
        "shift" | "maj" => Some(8),
        _ => None,
    }
}

fn modificateurs(args: &Value) -> Result<u32, String> {
    let mut m = 0;
    for v in args.get("modifiers").and_then(Value::as_array).into_iter().flatten() {
        let n = v.as_str().unwrap_or_default();
        m |= modificateur(n).ok_or_else(|| format!("modificateur inconnu : {n} (ctrl, shift, alt, cmd)"))?;
    }
    Ok(m)
}

fn bouton(args: &Value) -> Result<(&'static str, u32), String> {
    match args.get("button").and_then(Value::as_str).unwrap_or("left") {
        "left" | "gauche" => Ok(("left", 1)),
        "right" | "droit" => Ok(("right", 2)),
        "middle" | "milieu" => Ok(("middle", 4)),
        b => Err(format!("bouton inconnu : {b} (left, right, middle)")),
    }
}

#[allow(clippy::too_many_arguments)]
fn souris(
    app: &AppHandle,
    type_: &str,
    x: f64,
    y: f64,
    bouton: &str,
    boutons: u32,
    clics: u32,
    modifs: u32,
) -> Result<(), String> {
    appeler(
        app,
        "Input.dispatchMouseEvent",
        json!({
            "type": type_, "x": x, "y": y, "button": bouton, "buttons": boutons,
            "clickCount": clics, "modifiers": modifs,
        }),
    )
    .map(|_| ())
}

fn cliquer(app: &AppHandle, x: f64, y: f64, args: &Value) -> Result<Reply, String> {
    let (b, masque) = bouton(args)?;
    let m = modificateurs(args)?;
    let double = args.get("double").and_then(Value::as_bool).unwrap_or(false);
    souris(app, "mouseMoved", x, y, "none", 0, 0, m)?;
    for clics in 1..=if double { 2 } else { 1 } {
        souris(app, "mousePressed", x, y, b, masque, clics, m)?;
        souris(app, "mouseReleased", x, y, b, 0, clics, m)?;
    }
    // Laisse le temps à l'interface (Svelte) de se mettre à jour.
    std::thread::sleep(Duration::from_millis(150));
    let quoi = if double { "double clic" } else { "clic" };
    Ok(Reply::Text(format!("ok : {quoi} en ({x:.0}, {y:.0})")))
}

fn requete(args: &Value) -> Value {
    let mut q = json!({});
    for k in ["label", "role", "index", "id", "selector"] {
        if let Some(v) = args.get(k) {
            q[k] = v.clone();
        }
    }
    q
}

fn viser(app: &AppHandle, args: &Value) -> Result<(f64, f64, String), String> {
    let v = mcp_js(app, &format!("window.__mcp.viser({})", requete(args)))?;
    let x = v.get("x").and_then(Value::as_f64).ok_or("position introuvable")?;
    let y = v.get("y").and_then(Value::as_f64).ok_or("position introuvable")?;
    Ok((x, y, v.get("desc").and_then(Value::as_str).unwrap_or_default().to_string()))
}

fn arbre(app: &AppHandle, args: &Value) -> Result<Reply, String> {
    let filtre = args.get("filter").and_then(Value::as_str).unwrap_or("");
    let tous = args.get("all").and_then(Value::as_bool).unwrap_or(false);
    let v = mcp_js(app, &format!("window.__mcp.arbre({}, {tous})", json!(filtre)))?;
    Ok(Reply::Text(v.as_str().unwrap_or_default().to_string()))
}

fn attendre(app: &AppHandle, args: &Value) -> Result<Reply, String> {
    let disparu = args.get("gone").and_then(Value::as_bool).unwrap_or(false);
    let ms = args.get("timeout_ms").and_then(Value::as_u64).unwrap_or(5000).min(25_000);
    let fin = Instant::now() + Duration::from_millis(ms);
    let appel = format!("window.__mcp.present({})", requete(args));
    loop {
        let present = mcp_js(app, &appel)?.as_bool().unwrap_or(false);
        if present != disparu {
            return Ok(Reply::Text(if disparu { "élément disparu" } else { "élément présent" }.into()));
        }
        if Instant::now() >= fin {
            return Err("délai dépassé : condition non remplie".into());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn glisser(app: &AppHandle, args: &Value) -> Result<Reply, String> {
    let (x, y) = position(args, "x", "y")?;
    let (x2, y2) = position(args, "to_x", "to_y")?;
    let (b, masque) = bouton(args)?;
    let m = modificateurs(args)?;
    let n = args.get("steps").and_then(Value::as_u64).unwrap_or(10).clamp(1, 60);
    souris(app, "mouseMoved", x, y, "none", 0, 0, m)?;
    souris(app, "mousePressed", x, y, b, masque, 1, m)?;
    for i in 1..=n {
        let t = i as f64 / n as f64;
        souris(app, "mouseMoved", x + (x2 - x) * t, y + (y2 - y) * t, b, masque, 0, m)?;
        std::thread::sleep(Duration::from_millis(16));
    }
    souris(app, "mouseReleased", x2, y2, b, 0, 1, m)?;
    Ok(Reply::Text(format!("ok : glisser ({x:.0}, {y:.0}) → ({x2:.0}, {y2:.0})")))
}

fn defiler(app: &AppHandle, args: &Value) -> Result<Reply, String> {
    let (x, y) = position(args, "x", "y")?;
    let dx = args.get("dx").and_then(Value::as_f64).unwrap_or(0.0);
    let dy = args.get("dy").and_then(Value::as_f64).unwrap_or(0.0);
    if dx == 0.0 && dy == 0.0 {
        return Err("indiquer dx et/ou dy (pixels CSS)".into());
    }
    // Même convention que la version egui : dy négatif fait descendre le contenu.
    appeler(
        app,
        "Input.dispatchMouseEvent",
        json!({
            "type": "mouseWheel", "x": x, "y": y, "deltaX": -dx, "deltaY": -dy,
            "modifiers": modificateurs(args)?,
        }),
    )?;
    std::thread::sleep(Duration::from_millis(150));
    Ok(Reply::Text(format!("ok : défilement de ({dx}, {dy})")))
}

fn saisir(app: &AppHandle, args: &Value) -> Result<Reply, String> {
    let texte = args.get("text").and_then(Value::as_str).ok_or("argument « text » manquant")?;
    for (i, morceau) in texte.split('\n').enumerate() {
        if i > 0 {
            envoyer_touche(app, "Enter", 0)?;
        }
        if !morceau.is_empty() {
            appeler(app, "Input.insertText", json!({ "text": morceau }))?;
        }
    }
    std::thread::sleep(Duration::from_millis(100));
    Ok(Reply::Text(format!("ok : saisie de {} caractère(s)", texte.chars().count())))
}

/// (key, code, code de touche virtuelle Windows, texte produit).
fn infos_touche(nom: &str) -> Option<(String, String, u32, Option<String>)> {
    let bas = nom.to_lowercase();
    let simple = |k: &str, vk: u32| Some((k.to_string(), k.to_string(), vk, None));
    match bas.as_str() {
        "enter" | "entree" | "entrée" | "return" => Some(("Enter".into(), "Enter".into(), 13, Some("\r".into()))),
        "escape" | "esc" | "echap" | "échap" => simple("Escape", 27),
        "tab" => simple("Tab", 9),
        "backspace" | "retour" => simple("Backspace", 8),
        "delete" | "del" | "suppr" => simple("Delete", 46),
        "arrowup" | "up" | "haut" => simple("ArrowUp", 38),
        "arrowdown" | "down" | "bas" => simple("ArrowDown", 40),
        "arrowleft" | "left" | "gauche" => simple("ArrowLeft", 37),
        "arrowright" | "right" | "droite" => simple("ArrowRight", 39),
        "home" | "debut" | "début" => simple("Home", 36),
        "end" | "fin" => simple("End", 35),
        "pageup" | "pgup" => simple("PageUp", 33),
        "pagedown" | "pgdn" => simple("PageDown", 34),
        "space" | "espace" => Some((" ".into(), "Space".into(), 32, Some(" ".into()))),
        _ => {
            if let Some(n) = bas.strip_prefix('f').and_then(|n| n.parse::<u32>().ok()) {
                if (1..=12).contains(&n) {
                    return Some((format!("F{n}"), format!("F{n}"), 111 + n, None));
                }
            }
            let mut c = nom.chars();
            let (Some(ch), None) = (c.next(), c.next()) else { return None };
            if ch.is_ascii_alphabetic() {
                let up = ch.to_ascii_uppercase();
                Some((ch.to_string(), format!("Key{up}"), up as u32, Some(ch.to_string())))
            } else if ch.is_ascii_digit() {
                Some((ch.to_string(), format!("Digit{ch}"), ch as u32, Some(ch.to_string())))
            } else {
                // Ponctuation : texte seul (pas de code de touche fiable).
                Some((ch.to_string(), String::new(), 0, Some(ch.to_string())))
            }
        }
    }
}

/// « Ctrl+Shift+S » → (S, masque).
fn analyser_touche(spec: &str) -> Result<(String, u32), String> {
    // « Ctrl++ » ou « + » : la touche elle-même est « + ».
    let (prefixe, touche) = match spec.strip_suffix("++") {
        Some(p) => (p, "+"),
        None if spec == "+" => ("", "+"),
        None => match spec.rsplit_once('+') {
            Some((p, t)) => (p, t.trim()),
            None => ("", spec.trim()),
        },
    };
    if touche.is_empty() {
        return Err("touche vide".into());
    }
    let mut m = 0;
    for p in prefixe.split('+').map(str::trim).filter(|p| !p.is_empty()) {
        m |= modificateur(p).ok_or_else(|| format!("modificateur inconnu : {p}"))?;
    }
    Ok((touche.to_string(), m))
}

fn envoyer_touche(app: &AppHandle, nom: &str, modifs: u32) -> Result<(), String> {
    let (key, code, vk, texte) = infos_touche(nom).ok_or_else(|| format!("touche inconnue : {nom}"))?;
    // Avec Ctrl, Alt ou Meta, pas de caractère produit (raccourci).
    let texte = texte.filter(|_| modifs & 0b111 == 0);
    let key = if modifs & 8 != 0 && key.len() == 1 { key.to_uppercase() } else { key };
    let mut bas = json!({
        "type": if texte.is_some() { "keyDown" } else { "rawKeyDown" },
        "key": key, "code": code, "windowsVirtualKeyCode": vk, "nativeVirtualKeyCode": vk,
        "modifiers": modifs,
    });
    if let Some(t) = &texte {
        let t = if modifs & 8 != 0 { t.to_uppercase() } else { t.clone() };
        bas["text"] = json!(t);
        bas["unmodifiedText"] = json!(t);
    }
    appeler(app, "Input.dispatchKeyEvent", bas)?;
    appeler(
        app,
        "Input.dispatchKeyEvent",
        json!({
            "type": "keyUp", "key": key, "code": code, "windowsVirtualKeyCode": vk,
            "nativeVirtualKeyCode": vk, "modifiers": modifs,
        }),
    )
    .map(|_| ())
}

fn touche(app: &AppHandle, args: &Value) -> Result<Reply, String> {
    let spec = args
        .get("key")
        .and_then(Value::as_str)
        .ok_or("argument « key » manquant (ex. « Enter », « Ctrl+S », « F5 »)")?;
    let (nom, m) = analyser_touche(spec)?;
    let m = m | modificateurs(args)?;
    let fois = args.get("times").and_then(Value::as_u64).unwrap_or(1).clamp(1, 50);
    for _ in 0..fois {
        envoyer_touche(app, &nom, m)?;
    }
    std::thread::sleep(Duration::from_millis(120));
    Ok(Reply::Text(format!("ok : touche {spec}")))
}

fn invoquer(app: &AppHandle, args: &Value) -> Result<Reply, String> {
    let commande = args.get("command").and_then(Value::as_str).ok_or("argument « command » manquant")?;
    if !COMMANDES_LECTURE.contains(&commande) {
        return Err(format!(
            "commande « {commande} » non autorisée (lecture seule) : {}",
            COMMANDES_LECTURE.join(", ")
        ));
    }
    let params = args.get("args").cloned().unwrap_or_else(|| json!({}));
    evaluer(app, &format!("window.__TAURI_INTERNALS__.invoke({}, {params})", json!(commande))).map(Reply::Json)
}

fn capture(app: &AppHandle, args: &Value) -> Reply {
    let resultat = (|| -> Result<Reply, String> {
        let m = evaluer(app, "({ l: innerWidth, h: innerHeight, r: devicePixelRatio })")?;
        let num = |v: &Value, k: &str| v.get(k).and_then(Value::as_f64);
        let (l, h) = (num(&m, "l").unwrap_or(1280.0), num(&m, "h").unwrap_or(800.0));
        let ratio = num(&m, "r").filter(|r| *r > 0.0).unwrap_or(1.0);
        let (x, y, w, hh) = match (num(args, "x"), num(args, "y"), num(args, "width"), num(args, "height")) {
            (Some(x), Some(y), Some(w), Some(hh)) if w > 0.0 && hh > 0.0 => (x, y, w, hh),
            _ => (0.0, 0.0, l, h),
        };
        let natif = args.get("native").and_then(Value::as_bool).unwrap_or(false);
        let max = args.get("max_width").and_then(Value::as_f64).unwrap_or(1600.0).clamp(64.0, 4096.0);
        // Pixels de sortie = largeur CSS × échelle × ratio : 1 px = 1 pixel CSS par défaut.
        let mut echelle = if natif { 1.0 } else { 1.0 / ratio };
        if w * echelle * ratio > max {
            echelle = max / (w * ratio);
        }
        let r = appeler(
            app,
            "Page.captureScreenshot",
            json!({
                "format": "png",
                "fromSurface": true,
                "clip": { "x": x, "y": y, "width": w, "height": hh, "scale": echelle },
            }),
        )?;
        let data = r.get("data").and_then(Value::as_str).ok_or("capture vide")?;
        let px = (w * echelle * ratio).round();
        let pxh = (hh * echelle * ratio).round();
        let par_px = 1.0 / (echelle * ratio);
        let texte = if (par_px - 1.0).abs() < 0.01 && x == 0.0 && y == 0.0 {
            format!("Capture {px}×{pxh} px ; 1 px = 1 pixel CSS (coordonnées utilisables telles quelles pour cliquer).")
        } else {
            format!("Capture {px}×{pxh} px ; position = ({x:.0} + px_x × {par_px:.3}, {y:.0} + px_y × {par_px:.3}).")
        };
        Ok(Reply::Image { png_base64: data.to_string(), text: texte })
    })();
    resultat.unwrap_or_else(Reply::Error)
}

fn schema(props: Value, requis: &[&str]) -> Value {
    json!({ "type": "object", "properties": props, "required": requis })
}

fn fusion(mut a: Value, b: Value) -> Value {
    if let (Some(a), Value::Object(b)) = (a.as_object_mut(), b) {
        a.extend(b);
    }
    a
}

/// Outils déclarés au client.
pub fn liste() -> Vec<Tool> {
    let xy = || {
        json!({
            "x": { "type": "number", "description": "abscisse en pixels CSS (1 px de capture = 1 pixel CSS)" },
            "y": { "type": "number", "description": "ordonnée en pixels CSS" },
        })
    };
    let clic = || {
        json!({
            "button": { "type": "string", "enum": ["left", "right", "middle"] },
            "double": { "type": "boolean" },
            "modifiers": { "type": "array", "items": { "type": "string", "enum": ["ctrl", "shift", "alt", "cmd"] } },
        })
    };
    let cible = || {
        json!({
            "label": { "type": "string", "description": "texte de l'élément (exact de préférence, sinon contenu ; insensible à la casse)" },
            "role": { "type": "string", "description": "rôle facultatif : button, link, textbox, checkbox, option, treeitem…" },
            "index": { "type": "integer", "description": "rang parmi les éléments trouvés (0 par défaut)" },
            "selector": { "type": "string", "description": "sélecteur CSS (à la place de label)" },
            "id": { "type": "string", "description": "id affiché par ui_tree" },
        })
    };
    vec![
        Tool {
            name: "screenshot",
            description: "Capture la fenêtre (PNG). Par défaut, 1 pixel = 1 pixel CSS : les coordonnées lues \
                sur l'image servent directement à click. Zone facultative (x, y, width, height).",
            input_schema: schema(
                fusion(
                    xy(),
                    json!({
                        "width": { "type": "number" }, "height": { "type": "number" },
                        "native": { "type": "boolean", "description": "résolution physique" },
                        "max_width": { "type": "integer", "description": "1600 par défaut" },
                    }),
                ),
                &[],
            ),
        },
        Tool {
            name: "ui_tree",
            description: "Liste les éléments visibles de la page (rôle, libellé, valeur, position, état). \
                `filter` restreint par texte ; `all` inclut tous les éléments feuilles.",
            input_schema: schema(json!({ "filter": { "type": "string" }, "all": { "type": "boolean" } }), &[]),
        },
        Tool {
            name: "get_state",
            description: "État de l'interface : section, mode, nombre de livres, liste courante, dialogues \
                ouverts, alertes, élément actif, erreurs JavaScript récentes.",
            input_schema: schema(json!({}), &[]),
        },
        Tool {
            name: "click_widget",
            description:
                "Clique au centre d'un élément trouvé par libellé, sélecteur CSS ou id (amené à l'écran si besoin).",
            input_schema: schema(fusion(cible(), clic()), &[]),
        },
        Tool {
            name: "wait_for",
            description: "Attend qu'un élément apparaisse (ou disparaisse avec gone=true).",
            input_schema: schema(
                fusion(cible(), json!({ "gone": { "type": "boolean" }, "timeout_ms": { "type": "integer" } })),
                &[],
            ),
        },
        Tool {
            name: "click",
            description: "Clic à une position (pixels CSS).",
            input_schema: schema(fusion(xy(), clic()), &["x", "y"]),
        },
        Tool {
            name: "hover",
            description: "Déplace le pointeur (survol ; hold_ms pour laisser apparaître une info-bulle).",
            input_schema: schema(fusion(xy(), json!({ "hold_ms": { "type": "integer" } })), &["x", "y"]),
        },
        Tool {
            name: "drag",
            description: "Glisser de (x, y) à (to_x, to_y) avec la souris (événements pointeur ; le \
                glisser-déposer HTML natif peut ne pas réagir).",
            input_schema: schema(
                fusion(
                    xy(),
                    json!({
                        "to_x": { "type": "number" }, "to_y": { "type": "number" }, "steps": { "type": "integer" },
                        "button": { "type": "string" }, "modifiers": { "type": "array", "items": { "type": "string" } },
                    }),
                ),
                &["x", "y", "to_x", "to_y"],
            ),
        },
        Tool {
            name: "scroll",
            description: "Molette au-dessus de (x, y). dy négatif : descendre ; dx : horizontal (pixels CSS).",
            input_schema: schema(
                fusion(xy(), json!({ "dx": { "type": "number" }, "dy": { "type": "number" } })),
                &["x", "y"],
            ),
        },
        Tool {
            name: "type_text",
            description: "Saisit du texte dans l'élément qui a le focus (cliquer d'abord dans le champ). \
                Les retours à la ligne envoient Entrée.",
            input_schema: schema(json!({ "text": { "type": "string" } }), &["text"]),
        },
        Tool {
            name: "press_key",
            description:
                "Touche ou raccourci : « Enter », « Escape », « Tab », « F5 », « Ctrl+A », « Shift+ArrowDown »…",
            input_schema: schema(
                json!({
                    "key": { "type": "string" }, "times": { "type": "integer" },
                    "modifiers": { "type": "array", "items": { "type": "string" } },
                }),
                &["key"],
            ),
        },
        Tool {
            name: "wait",
            description: "Attend un délai (ms, 500 par défaut, 20000 au plus).",
            input_schema: schema(json!({ "ms": { "type": "integer" } }), &[]),
        },
        Tool {
            name: "get_logs",
            description: "Dernières lignes de la console de la page et des erreurs JavaScript.",
            input_schema: schema(json!({ "lines": { "type": "integer" }, "filter": { "type": "string" } }), &[]),
        },
        Tool {
            name: "eval_js",
            description: "Évalue une expression JavaScript dans la page et renvoie sa valeur (promesses attendues). \
                Sans restriction : réservé au débogage (lire le DOM, l'état, les styles…).",
            input_schema: schema(json!({ "expression": { "type": "string" } }), &["expression"]),
        },
        Tool {
            name: "invoke",
            description: "Appelle une commande du backend Rust en lecture seule (infos, lister_livres, \
                detail_livre, arbre_listes, etat_listes, lire_fiches…) et renvoie sa réponse JSON.",
            input_schema: schema(
                json!({
                    "command": { "type": "string", "enum": COMMANDES_LECTURE },
                    "args": { "type": "object", "description": "arguments de la commande (noms camelCase du front)" },
                }),
                &["command"],
            ),
        },
        Tool {
            name: "reload_page",
            description: "Recharge l'interface web (le backend reste lancé).",
            input_schema: schema(json!({}), &[]),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn touches() {
        assert_eq!(infos_touche("Enter").unwrap().2, 13);
        assert_eq!(infos_touche("f5").unwrap().0, "F5");
        assert_eq!(infos_touche("a").unwrap().1, "KeyA");
        assert_eq!(infos_touche("Échap").unwrap().0, "Escape");
        assert!(infos_touche("Hyper").is_none());
        assert_eq!(analyser_touche("Ctrl+Shift+s").unwrap(), ("s".into(), 10));
        assert_eq!(analyser_touche("Ctrl++").unwrap(), ("+".into(), 2));
        assert!(analyser_touche("Truc+A").is_err());
    }

    #[test]
    fn outils_uniques() {
        let l = liste();
        let mut noms: Vec<_> = l.iter().map(|t| t.name).collect();
        noms.sort_unstable();
        noms.dedup();
        assert_eq!(noms.len(), l.len());
    }
}

//! Formulaires et listes de choix des scripts **vus par le front** : `FormSpec` / `ChooseSpec`
//! (déjà vérifiés et assainis par `comicrust_core::scripting::forms`) mis en JSON (camelCase,
//! images en URL `data:`), et réponses du front relues (`script_repondre`). Format détaillé dans
//! `docs/tauri/ipc-scripts.md`. Aucune dépendance à Tauri.

use comicrust_core::scripting::forms::{
    image_format, ChoiceItem, ChooseLayout, ColumnKind, Condition, Control, ControlKind, InsertGroup, LabelStyle,
};
use comicrust_core::scripting::{ChooseSpec, FormAnswer, FormSpec, LazyImage, Question, QuestionKind};
use serde_json::{json, Map, Value};

// ───────────────────────────── Images ─────────────────────────────

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Base64 standard (avec `=`), pour les URL `data:`.
pub fn base64(octets: &[u8]) -> String {
    let mut out = String::with_capacity(octets.len().div_ceil(3) * 4);
    for bloc in octets.chunks(3) {
        let b = [bloc[0], *bloc.get(1).unwrap_or(&0), *bloc.get(2).unwrap_or(&0)];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            if i <= bloc.len() {
                out.push(ALPHABET[((n >> (18 - 6 * i)) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// URL `data:` d'une image (format reconnu d'après ses premiers octets ; inconnu : `None`).
pub fn url_image(octets: &[u8]) -> Option<String> {
    let mime = match image_format(octets)? {
        "JPEG" => "image/jpeg",
        "PNG" => "image/png",
        "WebP" => "image/webp",
        "GIF" => "image/gif",
        "BMP" => "image/bmp",
        _ => return None,
    };
    Some(format!("data:{mime};base64,{}", base64(octets)))
}

// ───────────────────────────── Questions ─────────────────────────────

/// Détail d'une question `comicrust.ask` / `ask_choice` / `confirm` (champ `question` de
/// l'événement `script:question`). Saisie secrète : aucune valeur par défaut transmise.
pub fn question_json(q: &Question) -> Value {
    let genre = match q.kind {
        QuestionKind::Text => "texte",
        QuestionKind::Choice => "choix",
        QuestionKind::Confirm => "confirmation",
    };
    let defaut = if q.secret { Value::Null } else { q.default.clone() };
    json!({"type": genre, "defaut": defaut, "choix": q.choices, "secret": q.secret})
}

// ───────────────────────────── Formulaires ─────────────────────────────
//
// Le front lit le JSON **au format des scripts** (`comicrust.form` : `type`, `key`, `label`,
// `enabled_if`, `default`, `choices`…), tel que validé et assaini par le cœur : valeurs par
// défaut corrigées, choix normalisés en `{"value", "label"}`, images en URL `data:` (au lieu du
// base64 du script), `id` (numéro unique) ajouté à chaque contrôle.

fn condition_texte(c: &Condition) -> String {
    match (&c.equals, c.negate) {
        (Some(v), false) => format!("{}={v}", c.key),
        (Some(v), true) => format!("{}!={v}", c.key),
        (None, false) => c.key.clone(),
        (None, true) => format!("!{}", c.key),
    }
}

fn choix_json(choix: &[ChoiceItem]) -> Value {
    Value::Array(choix.iter().map(|c| json!({"value": c.value, "label": c.label})).collect())
}

fn insertions_json(groupes: &[InsertGroup]) -> Value {
    Value::Array(groupes.iter().map(|g| json!({"group": g.title, "items": choix_json(&g.items)})).collect())
}

fn style_json(s: LabelStyle) -> &'static str {
    match s {
        LabelStyle::Normal => "normal",
        LabelStyle::Bold => "bold",
        LabelStyle::Small => "small",
        LabelStyle::Weak => "weak",
        LabelStyle::Heading => "heading",
        LabelStyle::Warning => "warning",
        LabelStyle::Error => "error",
    }
}

fn colonne_json(k: ColumnKind) -> &'static str {
    match k {
        ColumnKind::Text => "text",
        ColumnKind::Number => "number",
        ColumnKind::Checkbox => "checkbox",
        ColumnKind::Choice => "choice",
    }
}

fn controles_json(cs: &[Control]) -> Value {
    Value::Array(cs.iter().map(controle_json).collect())
}

/// Un contrôle : champs communs puis champs propres à son `type` (noms de `comicrust.form`).
fn controle_json(c: &Control) -> Value {
    let mut o = Map::new();
    o.insert("id".into(), json!(c.id));
    o.insert("key".into(), json!(c.key));
    o.insert("label".into(), json!(c.label));
    o.insert("help".into(), json!(c.help));
    if let Some(e) = &c.enabled_if {
        o.insert("enabled_if".into(), json!(condition_texte(e)));
    }
    if let Some(w) = c.width {
        o.insert("width".into(), json!(w));
    }
    let (genre, propre) = match &c.kind {
        ControlKind::Text { default, placeholder, required, rows, secret, inserts, paste, hints, hint_none } => (
            if *secret { "secret" } else { "text" },
            json!({
                // Saisie secrète : jamais pré-remplie.
                "default": if *secret { "" } else { default.as_str() },
                "placeholder": placeholder, "required": required, "rows": rows, "secret": secret,
                "inserts": insertions_json(inserts), "paste": paste,
                "hints": hints.iter().map(|h| json!([h.pattern, h.text])).collect::<Vec<_>>(),
                "hint_none": hint_none,
            }),
        ),
        ControlKind::Checkbox { default } => ("checkbox", json!({ "default": default })),
        ControlKind::Number { default, min, max, step, decimals } => {
            ("number", json!({"default": default, "min": min, "max": max, "step": step, "decimals": decimals}))
        }
        ControlKind::Choice { choices, default, editable, required } => (
            "choice",
            json!({"choices": choix_json(choices), "default": default, "editable": editable, "required": required}),
        ),
        ControlKind::Radio { choices, default, horizontal } => {
            ("radio", json!({"choices": choix_json(choices), "default": default, "horizontal": horizontal}))
        }
        ControlKind::MultiChoice { choices, default, height, required } => (
            "multichoice",
            json!({"choices": choix_json(choices), "default": default, "height": height, "required": required}),
        ),
        ControlKind::ListEdit { default, choices, unique, required, height } => (
            "list_edit",
            json!({"default": default, "choices": choices, "unique": unique, "required": required, "height": height}),
        ),
        ControlKind::Table { columns, default, height } => (
            "table",
            json!({
                "columns": columns.iter().map(|k| json!({
                    "title": k.title, "type": colonne_json(k.kind), "choices": choix_json(&k.choices), "width": k.width,
                })).collect::<Vec<_>>(),
                "default": default, "height": height,
            }),
        ),
        ControlKind::Path { default, folder, save, filters, required } => (
            if *folder { "folder" } else { "file" },
            json!({
                "default": default, "save": save, "required": required,
                "filters": filters.iter().map(|(n, e)| json!([n, e])).collect::<Vec<_>>(),
            }),
        ),
        ControlKind::Label { text, style } => ("label", json!({"text": text, "style": style_json(*style)})),
        ControlKind::Separator => ("separator", json!({})),
        ControlKind::Image { data, caption, max_width, max_height } => (
            "image",
            json!({"data": url_image(data), "caption": caption, "max_width": max_width, "max_height": max_height}),
        ),
        ControlKind::Group { title, controls } => {
            ("group", json!({"title": title, "controls": controles_json(controls)}))
        }
        ControlKind::Columns { columns, widths } => (
            "columns",
            json!({"columns": columns.iter().map(|c| controles_json(c)).collect::<Vec<_>>(), "widths": widths}),
        ),
        ControlKind::Tabs { tabs } => (
            "tabs",
            json!({"tabs": tabs.iter().map(|(t, c)| json!({"title": t, "controls": controles_json(c)}))
                .collect::<Vec<_>>()}),
        ),
        ControlKind::Button { validate } => ("button", json!({ "validate": validate })),
    };
    o.insert("type".into(), json!(genre));
    if let Value::Object(p) = propre {
        o.extend(p);
    }
    Value::Object(o)
}

/// Formulaire pour le front (événement `script:formulaire`, champ `spec`), au format de
/// `comicrust.form`. `values` : valeurs initiales calculées par le cœur (défauts, puis valeurs
/// données par le script ; saisies secrètes jamais pré-remplies) ; `cancel` : bouton
/// d'annulation : son libellé (ou `null`).
pub fn formulaire_json(spec: &FormSpec) -> Value {
    json!({
        "title": spec.title,
        "message": spec.message,
        "controls": controles_json(&spec.controls),
        "buttons": spec.buttons,
        "cancel": spec.cancel.and_then(|i| spec.buttons.get(i)),
        "width": spec.width,
        "label_width": spec.label_width,
        "values": spec.initial_values(),
    })
}

/// Clé réservée de la réponse d'un formulaire : libellé du bouton du bas, ou clé d'un bouton du
/// formulaire.
pub const CLE_BOUTON: &str = "$bouton";

/// Réponse du front à un formulaire : `null` = annulé ; sinon l'objet des valeurs avec la clé
/// réservée [`CLE_BOUTON`] (absente : premier bouton du bas). Le cœur vérifie ensuite le
/// bouton et assainit les valeurs.
pub fn reponse_formulaire(spec: &FormSpec, v: &Value) -> Option<FormAnswer> {
    let mut values = v.as_object()?.clone();
    let button = match values.remove(CLE_BOUTON) {
        Some(Value::String(b)) => b,
        _ => spec.buttons.first().cloned().unwrap_or_else(|| "OK".into()),
    };
    Some(FormAnswer { button, values })
}

// ───────────────────────────── Listes de choix ─────────────────────────────

/// Liste de choix pour le front (événement `script:choix`, champ `spec`), au format de
/// `comicrust.choose` : `images` = une URL `data:` (ou `null`) par ligne, vide sans vignettes ;
/// `lazy` / `on_demand` = rangs des vignettes envoyées ensuite par `script:vignette`.
pub fn choix_json_spec(spec: &ChooseSpec) -> Value {
    let rangs = |genre: LazyImage| -> Vec<usize> {
        spec.lazy.iter().enumerate().filter(|(_, l)| **l == genre).map(|(i, _)| i).collect()
    };
    json!({
        "title": spec.title,
        "message": spec.message,
        "columns": spec.columns,
        "rows": spec.rows,
        "images": spec.images.iter().map(|i| i.as_deref().and_then(|b| url_image(b))).collect::<Vec<_>>(),
        "multi": spec.multi,
        "default": spec.default,
        "filter": spec.filter,
        "timeout": spec.timeout,
        "thumb_size": spec.thumb_size,
        "layout": match spec.layout { ChooseLayout::Table => "table", ChooseLayout::Cards => "cards" },
        "lazy": rangs(LazyImage::Auto),
        "on_demand": rangs(LazyImage::OnDemand),
    })
}

/// Réponse du front à une liste de choix : rang, liste de rangs, ou `null` (annulé).
pub fn reponse_choix(v: &Value) -> Option<Vec<usize>> {
    match v {
        Value::Number(n) => n.as_u64().map(|i| vec![i as usize]),
        Value::Array(a) => Some(a.iter().filter_map(Value::as_u64).map(|i| i as usize).collect()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_comme_la_norme() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
        assert!(url_image(b"\x89PNG\r\n\x1a\nxxxx").unwrap().starts_with("data:image/png;base64,"));
        assert_eq!(url_image(b"pas une image"), None);
    }

    #[test]
    fn reponses_du_front() {
        let p: Map<String, Value> = serde_json::from_value(json!({"title": "T", "controls": [
            {"type": "text", "key": "nom", "enabled_if": "!c"},
            {"type": "checkbox", "key": "c"},
            {"type": "choice", "key": "l", "choices": ["a", ["b", "Bé"]], "default": "b"},
        ]}))
        .unwrap();
        let spec = FormSpec::parse(&p).unwrap();
        let f = formulaire_json(&spec);
        // Format des scripts, assaini par le cœur : relu tel quel par le cœur.
        assert_eq!(f["controls"][0]["type"], "text");
        assert_eq!(f["controls"][0]["key"], "nom");
        assert_eq!(f["controls"][0]["enabled_if"], "!c");
        assert_eq!(f["controls"][2]["choices"][1], json!({"value": "b", "label": "Bé"}));
        assert_eq!(f["values"]["l"], "b");
        let relu = FormSpec::parse(f.as_object().unwrap()).unwrap();
        assert_eq!(relu.controls, spec.controls);
        assert_eq!(reponse_formulaire(&spec, &Value::Null), None);
        let a = reponse_formulaire(&spec, &json!({"nom": "Yoyo"})).unwrap();
        assert_eq!(a.values["nom"], "Yoyo");
        assert_eq!(a.button, "OK", "sans bouton : premier bouton du bas");
        let a = reponse_formulaire(&spec, &json!({"$bouton": "Annuler", "nom": "x"})).unwrap();
        assert_eq!(a.button, "Annuler");
        assert!(!a.values.contains_key("$bouton") && a.values["nom"] == "x");
        assert_eq!(reponse_choix(&json!(2)), Some(vec![2]));
        assert_eq!(reponse_choix(&json!([0, 3])), Some(vec![0, 3]));
        assert_eq!(reponse_choix(&Value::Null), None);
    }
}

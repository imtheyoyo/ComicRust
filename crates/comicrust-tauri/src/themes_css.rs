//! Validation et assainissement du CSS d'un thème (voir `docs/tauri/themes.md`).
//!
//! Un thème est du CSS écrit par l'utilisateur (ou récupéré sur Internet) puis injecté dans la page :
//! il ne doit jamais pouvoir charger une ressource extérieure ni exécuter quoi que ce soit.
//!
//! # Principe
//!
//! 1. Refus immédiat : octet NUL, plus de 1 Mo, `</style` ou `<script` (sortie de l'élément `<style>`).
//! 2. Les caractères de contrôle (hors saut de ligne, retour chariot, tabulation) sont ôtés.
//! 3. Le texte est découpé en **unités** (un en-tête `sélecteur {`, une déclaration `propriété: valeur;`
//!    ou une accolade fermante) par un balayage qui connaît chaînes, commentaires, parenthèses et
//!    échappements, comme un navigateur.
//! 4. Chaque unité est analysée **après normalisation** : commentaires retirés (`ur/**/l(` devient
//!    `url(`), échappements CSS décodés (`\75rl(` devient `url(`), minuscules.
//!    - Une construction interdite écrite **en clair** est supprimée (l'unité entière, ou le bloc entier
//!      pour `@font-face`) et signalée.
//!    - Une construction interdite qui n'apparaît **qu'après décodage** est de l'obfuscation : le fichier
//!      entier est refusé (« en cas de doute, rejeter plutôt que réparer »).
//! 5. Le résultat est ré-analysé : il doit être stable (rien de plus à supprimer), sinon le fichier est refusé.
//!
//! Interdits : `@import`, `@font-face`, `@namespace`, `behavior:`, `-moz-binding:`, `expression(`,
//! `image-set(`, `image(`, `src(`, `cross-fade(` et toute `url(...)` sauf une image `data:` PNG, JPEG, WebP,
//! GIF ou AVIF en base64 de 64 Ko au plus (jamais SVG).

use crate::themes::CONTRAT;

/// Taille maximale d'un fichier de thème.
pub const TAILLE_MAX_CSS: usize = 1024 * 1024;
/// Taille maximale de la partie base64 d'une image `data:` admise.
pub const TAILLE_MAX_DATA_URL: usize = 64 * 1024;

/// Résultat de [`valider_css`].
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Rapport {
    /// CSS assaini, prêt à injecter. Vide si `erreurs` n'est pas vide.
    pub css_assaini: String,
    /// Problèmes non bloquants (lignes supprimées, variables manquantes…).
    pub avertissements: Vec<String>,
    /// Motifs de refus du fichier entier.
    pub erreurs: Vec<String>,
}

// ───────────────────────────────── Jetons interdits ─────────────────────────────────

#[derive(Clone, Copy)]
enum Forme {
    /// Sous-chaîne (fonctions, règles `@`).
    Texte,
    /// Nom de propriété exact suivi de `:` (évite `scroll-behavior:`).
    Propriete,
}

struct Jeton {
    cherche: &'static str,
    forme: Forme,
    /// Message d'avertissement quand l'unité est supprimée.
    message: &'static str,
}

const MESSAGE_URL: &str =
    "url() supprimé (seules les images data: png, jpeg, webp, gif, avif de 64 Ko au plus sont admises)";

const JETONS: &[Jeton] = &[
    Jeton { cherche: "@import", forme: Forme::Texte, message: "@import supprimé" },
    Jeton { cherche: "@font-face", forme: Forme::Texte, message: "@font-face supprimé (bloc entier)" },
    Jeton { cherche: "@namespace", forme: Forme::Texte, message: "@namespace supprimé" },
    Jeton { cherche: "behavior", forme: Forme::Propriete, message: "propriété behavior supprimée" },
    Jeton { cherche: "-moz-binding", forme: Forme::Propriete, message: "propriété -moz-binding supprimée" },
    Jeton { cherche: "expression(", forme: Forme::Texte, message: "expression() supprimée" },
    Jeton { cherche: "image-set(", forme: Forme::Texte, message: "image-set() supprimé" },
    Jeton { cherche: "image(", forme: Forme::Texte, message: "image() supprimé" },
    Jeton { cherche: "src(", forme: Forme::Texte, message: "src() supprimé" },
    Jeton { cherche: "cross-fade(", forme: Forme::Texte, message: "cross-fade() supprimé" },
    Jeton { cherche: "url(", forme: Forme::Texte, message: MESSAGE_URL },
];

fn est_caractere_ident(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '-' || c == '_'
}

impl Jeton {
    /// Nombre d'occurrences dans `s` (déjà en minuscules ASCII).
    fn compter(&self, s: &str) -> usize {
        match self.forme {
            Forme::Texte => s.matches(self.cherche).count(),
            Forme::Propriete => s
                .match_indices(self.cherche)
                .filter(|(i, _)| {
                    let avant = s[..*i].chars().next_back();
                    let apres = s[i + self.cherche.len()..].trim_start();
                    avant.is_none_or(|c| !est_caractere_ident(c)) && apres.starts_with(':')
                })
                .count(),
        }
    }
}

// ───────────────────────────────── Balayage en unités ─────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Genre {
    /// `sélecteur {` ou `@règle … {`.
    Entete,
    /// `propriété: valeur;`, `@règle …;` ou le reste avant `}` / la fin.
    Declaration,
    /// `}`.
    Fermeture,
}

#[derive(Debug, Clone)]
struct Unite {
    /// Texte d'origine (commentaires compris).
    brut: String,
    /// Texte sans les commentaires (celui que voit le navigateur).
    propre: String,
    /// Commentaires de l'unité, dans l'ordre.
    commentaires: Vec<String>,
    /// Numéro de ligne (à partir de 1) du premier caractère significatif.
    ligne: usize,
    genre: Genre,
}

#[derive(Default)]
struct Constructeur {
    brut: String,
    propre: String,
    commentaires: Vec<String>,
    ligne: Option<usize>,
}

impl Constructeur {
    fn pousser(&mut self, c: char, ligne: usize) {
        if self.ligne.is_none() && !c.is_whitespace() {
            self.ligne = Some(ligne);
        }
        self.brut.push(c);
        self.propre.push(c);
    }

    fn finir(&mut self, genre: Genre, sortie: &mut Vec<Unite>) {
        let c = std::mem::take(self);
        sortie.push(Unite {
            brut: c.brut,
            propre: c.propre,
            commentaires: c.commentaires,
            ligne: c.ligne.unwrap_or(1),
            genre,
        });
    }
}

/// Découpe le CSS en unités. Erreur (texte du motif) si le texte est ambigu : commentaire, chaîne
/// ou parenthèse non terminés.
fn decouper(texte: &str) -> Result<Vec<Unite>, String> {
    let c: Vec<char> = texte.chars().collect();
    let mut lignes = Vec::with_capacity(c.len());
    let mut l = 1usize;
    for &ch in &c {
        lignes.push(l);
        if ch == '\n' {
            l += 1;
        }
    }
    let mut sortie = Vec::new();
    let mut cons = Constructeur::default();
    let mut paren = 0usize;
    let mut i = 0usize;
    while i < c.len() {
        let ch = c[i];
        match ch {
            '/' if c.get(i + 1) == Some(&'*') => {
                let fin = (i + 2..c.len().saturating_sub(1)).find(|&j| c[j] == '*' && c[j + 1] == '/');
                let Some(fin) = fin else {
                    return Err(format!("commentaire non terminé (ligne {})", lignes[i]));
                };
                let commentaire: String = c[i..fin + 2].iter().collect();
                cons.brut.push_str(&commentaire);
                cons.commentaires.push(commentaire);
                i = fin + 2;
                continue;
            }
            '"' | '\'' => {
                cons.pousser(ch, lignes[i]);
                i += 1;
                loop {
                    let Some(&d) = c.get(i) else {
                        return Err("chaîne non terminée".to_string());
                    };
                    cons.pousser(d, lignes[i]);
                    i += 1;
                    if d == '\\' {
                        let Some(&e) = c.get(i) else {
                            return Err("chaîne non terminée".to_string());
                        };
                        cons.pousser(e, lignes[i]);
                        i += 1;
                    } else if d == ch || d == '\n' {
                        // Fin de chaîne (un saut de ligne non échappé termine aussi une chaîne cassée).
                        break;
                    }
                }
                continue;
            }
            '\\' => {
                // Une échappement et son caractère restent ensemble (`\;` ne coupe pas l'unité).
                cons.pousser(ch, lignes[i]);
                if let Some(&e) = c.get(i + 1) {
                    cons.pousser(e, lignes[i + 1]);
                    i += 1;
                }
            }
            '(' => {
                paren += 1;
                cons.pousser(ch, lignes[i]);
            }
            ')' => {
                paren = paren.saturating_sub(1);
                cons.pousser(ch, lignes[i]);
            }
            ';' if paren == 0 => {
                cons.pousser(ch, lignes[i]);
                cons.finir(Genre::Declaration, &mut sortie);
            }
            '{' if paren == 0 => {
                cons.pousser(ch, lignes[i]);
                cons.finir(Genre::Entete, &mut sortie);
            }
            '}' if paren == 0 => {
                if !cons.brut.is_empty() {
                    cons.finir(Genre::Declaration, &mut sortie);
                }
                cons.pousser(ch, lignes[i]);
                cons.finir(Genre::Fermeture, &mut sortie);
            }
            _ => cons.pousser(ch, lignes[i]),
        }
        i += 1;
    }
    if paren > 0 {
        return Err("parenthèse non fermée".to_string());
    }
    if !cons.brut.is_empty() {
        cons.finir(Genre::Declaration, &mut sortie);
    }
    Ok(sortie)
}

// ───────────────────────────────── Normalisation et analyse ─────────────────────────────────

/// Décode les échappements CSS (`\75`, `\000075 `, `\u`, continuation de ligne).
fn decoder_echappements(s: &str) -> String {
    let c: Vec<char> = s.chars().collect();
    let mut sortie = String::with_capacity(s.len());
    let mut i = 0;
    while i < c.len() {
        if c[i] != '\\' {
            sortie.push(c[i]);
            i += 1;
            continue;
        }
        i += 1;
        let Some(&d) = c.get(i) else { break };
        if d.is_ascii_hexdigit() {
            let mut v = 0u32;
            let mut n = 0;
            while n < 6 && i < c.len() && c[i].is_ascii_hexdigit() {
                v = v * 16 + c[i].to_digit(16).unwrap_or(0);
                i += 1;
                n += 1;
            }
            // Un seul espace (ou saut de ligne) termine l'échappement et est absorbé.
            if c.get(i) == Some(&'\r') && c.get(i + 1) == Some(&'\n') {
                i += 2;
            } else if matches!(c.get(i), Some(' ' | '\t' | '\n' | '\r' | '\x0c')) {
                i += 1;
            }
            sortie.push(char::from_u32(v).filter(|&x| x != '\0').unwrap_or('\u{fffd}'));
        } else if d == '\n' || d == '\r' {
            // Continuation de ligne : rien.
            i += if d == '\r' && c.get(i + 1) == Some(&'\n') { 2 } else { 1 };
        } else {
            sortie.push(d);
            i += 1;
        }
    }
    sortie
}

/// Vrai si `apres` (texte qui suit `url(`, en minuscules) est une image `data:` admise.
fn url_data_valide(apres: &str) -> bool {
    let s = apres.trim_start();
    let (guillemet, s) = match s.chars().next() {
        Some(q @ ('"' | '\'')) => (Some(q), &s[1..]),
        _ => (None, s),
    };
    let Some(s) = s.strip_prefix("data:image/") else { return false };
    let Some(s) = ["png", "jpeg", "webp", "gif", "avif"].iter().find_map(|t| s.strip_prefix(*t)) else {
        return false;
    };
    let Some(s) = s.strip_prefix(";base64,") else { return false };
    let n = s.bytes().take_while(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'/' | b'=')).count();
    if n == 0 || n > TAILLE_MAX_DATA_URL {
        return false;
    }
    let s = &s[n..];
    let s = match guillemet {
        Some(q) => match s.strip_prefix(q) {
            Some(x) => x,
            None => return false,
        },
        None => s,
    };
    s.trim_start().starts_with(')')
}

enum Verdict {
    /// Garder l'unité ; `propre` = écrire le texte sans commentaires (images `data:` autorisées).
    Garder {
        propre: bool,
    },
    Supprimer(&'static str),
    Rejeter(String),
}

fn evaluer(u: &Unite) -> Verdict {
    let p = u.propre.to_ascii_lowercase();
    let d = decoder_echappements(&u.propre).to_ascii_lowercase();
    for j in JETONS {
        if j.compter(&d) > j.compter(&p) {
            return Verdict::Rejeter(format!(
                "construction interdite masquée par une échappement CSS (« {} », ligne {})",
                j.cherche, u.ligne
            ));
        }
    }
    if let Some(j) = JETONS.iter().filter(|j| j.cherche != "url(").find(|j| j.compter(&p) > 0) {
        return Verdict::Supprimer(j.message);
    }
    if p.contains("url(") {
        let toutes_valides = p.match_indices("url(").all(|(i, m)| url_data_valide(&p[i + m.len()..]));
        return if toutes_valides { Verdict::Garder { propre: true } } else { Verdict::Supprimer(MESSAGE_URL) };
    }
    Verdict::Garder { propre: false }
}

/// Une passe d'assainissement sur un texte sans caractère de contrôle. Renvoie le CSS et les
/// avertissements de suppression, ou le motif de refus.
fn assainir(texte: &str) -> Result<(String, Vec<String>), String> {
    let unites = decouper(texte)?;
    let mut sortie = String::with_capacity(texte.len());
    let mut avertissements = Vec::new();
    let mut saut = 0usize;
    for u in unites {
        if saut > 0 {
            match u.genre {
                Genre::Entete => saut += 1,
                Genre::Fermeture => saut -= 1,
                Genre::Declaration => {}
            }
            continue;
        }
        match evaluer(&u) {
            Verdict::Garder { propre } => sortie.push_str(if propre { &u.propre } else { &u.brut }),
            Verdict::Supprimer(message) => {
                avertissements.push(format!("ligne {} : {message}", u.ligne));
                // Les commentaires (inertes) sont conservés : `@nom` et `@contrat` en tête de fichier.
                for c in &u.commentaires {
                    sortie.push_str(c);
                    sortie.push('\n');
                }
                if u.genre == Genre::Entete {
                    saut = 1;
                }
            }
            Verdict::Rejeter(motif) => return Err(motif),
        }
    }
    Ok((sortie, avertissements))
}

// ───────────────────────────────── Déclarations ─────────────────────────────────

/// Toutes les déclarations `nom: valeur` du CSS (noms en minuscules, valeurs sans `;` final).
/// Vide si le CSS est ambigu.
pub fn declarations(css: &str) -> Vec<(String, String)> {
    let Ok(unites) = decouper(css) else { return Vec::new() };
    unites
        .iter()
        .filter(|u| u.genre == Genre::Declaration)
        .filter_map(|u| {
            let t = u.propre.trim().trim_end_matches(';');
            let (nom, valeur) = t.split_once(':')?;
            let nom = nom.trim();
            if nom.is_empty() || !nom.chars().all(est_caractere_ident) {
                return None;
            }
            Some((nom.to_ascii_lowercase(), valeur.trim().to_string()))
        })
        .collect()
}

/// Les variables `--cr-*` définies par le CSS, dans l'ordre (la dernière définition l'emporte pour
/// le navigateur : [`valeur_variable`]).
pub fn variables_cr(css: &str) -> Vec<(String, String)> {
    declarations(css).into_iter().filter(|(n, _)| n.starts_with("--cr-")).collect()
}

/// Valeur effective (dernière définition) d'une variable dans la liste de [`variables_cr`].
pub fn valeur_variable<'a>(variables: &'a [(String, String)], nom: &str) -> Option<&'a str> {
    variables.iter().rev().find(|(n, _)| n == nom).map(|(_, v)| v.as_str())
}

// ───────────────────────────────── Point d'entrée ─────────────────────────────────

/// Valide et assainit le CSS d'un thème (voir la documentation du module).
pub fn valider_css(css: &str) -> Rapport {
    let mut r = Rapport::default();
    if css.len() > TAILLE_MAX_CSS {
        r.erreurs.push(format!("fichier trop gros ({} octets, maximum {TAILLE_MAX_CSS})", css.len()));
    }
    if css.contains('\0') {
        r.erreurs.push("le fichier contient un octet NUL".to_string());
    }
    let bas = css.to_ascii_lowercase();
    if bas.contains("</style") || bas.contains("<script") {
        r.erreurs.push("balise HTML (</style ou <script) interdite dans un thème".to_string());
    }
    if !r.erreurs.is_empty() {
        return r;
    }

    let nettoye: String = css.chars().filter(|&c| !(c.is_control() && !matches!(c, '\n' | '\r' | '\t'))).collect();
    let nettoye = nettoye.replace('\u{feff}', "");
    if nettoye.len() != css.len() {
        r.avertissements.push("caractères de contrôle supprimés".to_string());
    }

    let (sortie, mut retires) = match assainir(&nettoye) {
        Ok(v) => v,
        Err(motif) => {
            r.erreurs.push(motif);
            return r;
        }
    };
    // Contrôle : le résultat doit être stable. Sinon, doute : refus.
    match assainir(&sortie) {
        Ok((deuxieme, restes)) if deuxieme == sortie && restes.is_empty() => {}
        _ => {
            r.erreurs.push("assainissement instable : fichier refusé par prudence".to_string());
            return r;
        }
    }
    r.avertissements.append(&mut retires);
    r.avertissements.extend(avertissements_contrat(&sortie));
    r.css_assaini = sortie;
    r
}

/// Avertissements sur le contenu : variables obligatoires manquantes, valeurs vides, variables
/// inconnues, `color-scheme` absent.
fn avertissements_contrat(css: &str) -> Vec<String> {
    let decls = declarations(css);
    let vars: Vec<(String, String)> = decls.iter().filter(|(n, _)| n.starts_with("--cr-")).cloned().collect();
    let mut av = Vec::new();
    for v in CONTRAT.iter().filter(|v| v.obligatoire) {
        if valeur_variable(&vars, v.nom).is_none() {
            av.push(format!("variable obligatoire absente : {} (repli du thème par défaut)", v.nom));
        }
    }
    let mut deja: Vec<&str> = Vec::new();
    for (nom, _) in &vars {
        if deja.contains(&nom.as_str()) {
            continue;
        }
        deja.push(nom);
        if valeur_variable(&vars, nom).is_some_and(|v| v.trim().is_empty()) {
            av.push(format!("valeur vide : {nom}"));
        }
        if !CONTRAT.iter().any(|v| v.nom == nom) {
            av.push(format!("variable inconnue : {nom}"));
        }
    }
    if !decls.iter().any(|(n, v)| n == "color-scheme" && !v.is_empty()) {
        av.push(
            "color-scheme absent : les cases à cocher et barres de défilement natives peuvent mal s'afficher"
                .to_string(),
        );
    }
    av
}

#[cfg(test)]
mod tests {
    use super::*;

    /// CSS minimal valide pour les tests (toutes les variables obligatoires).
    fn base() -> String {
        let mut s = String::from(":root {\n  color-scheme: light;\n");
        for v in CONTRAT.iter().filter(|v| v.obligatoire) {
            s.push_str(&format!("  {}: #123456;\n", v.nom));
        }
        s.push_str("}\n");
        s
    }

    /// Valide `base()` + `ajout` et renvoie le rapport.
    fn avec(ajout: &str) -> Rapport {
        valider_css(&format!("{}{ajout}", base()))
    }

    fn sans_url(r: &Rapport) -> bool {
        !r.css_assaini.to_ascii_lowercase().contains("url(")
    }

    #[test]
    fn css_valide_passe_tel_quel() {
        let css = base();
        let r = valider_css(&css);
        assert!(r.erreurs.is_empty(), "{:?}", r.erreurs);
        assert_eq!(r.css_assaini, css);
        assert!(r.avertissements.is_empty(), "{:?}", r.avertissements);
    }

    #[test]
    fn refuse_nul_et_taille() {
        assert!(!valider_css("a{b:c}\0").erreurs.is_empty());
        let gros = format!("/* {} */", "x".repeat(TAILLE_MAX_CSS));
        let r = valider_css(&gros);
        assert!(!r.erreurs.is_empty());
        assert!(r.css_assaini.is_empty());
        // Juste sous la limite : accepté.
        let ok = format!("/*{}*/", "x".repeat(TAILLE_MAX_CSS - 4));
        assert!(valider_css(&ok).erreurs.is_empty());
    }

    #[test]
    fn refuse_balises_html() {
        assert!(!avec("/* </style><script>alert(1)</script> */").erreurs.is_empty());
        assert!(!avec("/* </STYLE> */").erreurs.is_empty());
        assert!(!avec("a { content: '<script' }").erreurs.is_empty());
    }

    #[test]
    fn retire_les_caracteres_de_controle() {
        let r = avec("a { color: red\u{1}\u{7f}\u{85}; }\n\tb { x: y }");
        assert!(r.erreurs.is_empty());
        assert!(r.css_assaini.contains("color: red;"), "{}", r.css_assaini);
        assert!(r.css_assaini.contains('\t'));
        assert!(r.avertissements.iter().any(|a| a.contains("contrôle")));
        // BOM en tête : retiré sans bruit parasite.
        let r = valider_css(&format!("\u{feff}{}", base()));
        assert!(r.erreurs.is_empty());
        assert!(!r.css_assaini.contains('\u{feff}'));
    }

    #[test]
    fn import_sous_toutes_ses_formes() {
        for forme in [
            "@import 'http://evil.example/x.css';",
            "@import \"//evil.example/x.css\";",
            "@import url(http://evil.example/x.css);",
            "@import url('x.css') screen;",
            "@IMPORT url(\"x.css\");",
            "@import\n  'x.css'\n  ;",
        ] {
            let r = valider_css(&format!("{forme}\n{}", base()));
            assert!(r.erreurs.is_empty(), "{forme} : {:?}", r.erreurs);
            assert!(!r.css_assaini.to_ascii_lowercase().contains("@import"), "{forme} : {}", r.css_assaini);
            assert!(sans_url(&r), "{forme}");
            assert!(r.avertissements.iter().any(|a| a.contains("@import")), "{forme}");
            // Le reste du thème est conservé.
            assert!(r.css_assaini.contains("--cr-fond"), "{forme}");
        }
    }

    #[test]
    fn import_dans_une_media_ou_sans_point_virgule() {
        // Sans `;` final, l'unité englobe l'en-tête suivant : le bloc suivant est sacrifié, jamais l'inverse.
        let r = valider_css("@import 'x.css'\n:root { --cr-fond: red; }\n.b { color: blue; }");
        assert!(r.erreurs.is_empty());
        assert!(!r.css_assaini.contains("@import"));
        assert!(r.css_assaini.contains(".b { color: blue; }"));
        let r = avec("@media (min-width: 1px) { @import 'x.css'; .c { color: red } }");
        assert!(!r.css_assaini.contains("@import"));
        assert!(r.css_assaini.contains(".c { color: red }"));
    }

    #[test]
    fn font_face_et_namespace() {
        let r = avec("@font-face { font-family: X; src: url(http://evil.example/f.woff); }\n.a { color: red; }");
        assert!(!r.css_assaini.contains("@font-face"));
        assert!(!r.css_assaini.contains("font-family: X"));
        assert!(sans_url(&r));
        assert!(r.css_assaini.contains(".a { color: red; }"));
        assert!(r.avertissements.iter().any(|a| a.contains("@font-face")));

        let r = avec("@namespace svg url(http://www.w3.org/2000/svg);\n.a { color: red; }");
        assert!(!r.css_assaini.contains("@namespace"));
        assert!(sans_url(&r));
        assert!(r.css_assaini.contains(".a { color: red; }"));
    }

    #[test]
    fn font_face_imbrique_dans_media() {
        let r = avec("@media print { @font-face { font-family: Y; src: local(Y); } .p { color: red } }\n.z{color:red}");
        assert!(!r.css_assaini.contains("font-face"));
        assert!(r.css_assaini.contains(".z{color:red}"));
    }

    #[test]
    fn proprietes_dangereuses() {
        for (decl, motif) in [
            ("behavior: url(x.htc);", "behavior"),
            ("  BEHAVIOR : something;", "behavior"),
            ("-moz-binding: url(chrome://x);", "moz-binding"),
            ("width: expression(alert(1));", "expression"),
            ("width: EXPRESSION(alert(1));", "expression"),
        ] {
            let r = avec(&format!(".a {{ {decl} color: red; }}\n.b {{ {decl} }}"));
            assert!(r.erreurs.is_empty(), "{decl}");
            let bas = r.css_assaini.to_ascii_lowercase();
            assert!(!bas.contains(motif), "{decl} : {}", r.css_assaini);
            assert!(!r.avertissements.is_empty(), "{decl}");
        }
    }

    #[test]
    fn scroll_behavior_legitime_conserve() {
        let r = avec("html { scroll-behavior: smooth; overscroll-behavior: contain; }");
        assert!(r.css_assaini.contains("scroll-behavior: smooth;"));
        assert!(r.css_assaini.contains("overscroll-behavior: contain;"));
    }

    #[test]
    fn fonctions_d_images_interdites() {
        for f in [
            "background: image-set('a.png' 1x);",
            "background: -webkit-image-set(\"a.png\" 1x);",
            "background: image('a.png');",
            "content: src('a.png');",
            "background: cross-fade(red, blue);",
            "background: -webkit-cross-fade(red, blue);",
        ] {
            let r = avec(&format!(".a {{ {f} }}\n.b {{ color: red }}"));
            assert!(r.erreurs.is_empty(), "{f} : {:?}", r.erreurs);
            assert!(!r.css_assaini.contains("a.png"), "{f}");
            assert!(!r.css_assaini.contains("cross-fade"), "{f}");
            assert!(r.css_assaini.contains(".b { color: red }"), "{f}");
        }
    }

    #[test]
    fn url_supprimee_avec_ou_sans_guillemets() {
        for u in [
            "url(http://evil.example/x.png)",
            "url(https://evil.example/x.png)",
            "url('http://evil.example/x.png')",
            "url(\"http://evil.example/x.png\")",
            "url(//evil.example/x.png)",
            "url( //evil.example/x.png )",
            "url(x.png)",
            "url(\"x.png\")",
            "URL(x.png)",
            "url(#masque)",
            "url(javascript:alert(1))",
            "url(ftp://h/x)",
            "url(data:text/html;base64,PHNjcmlwdD4=)",
            "url(\"data:text/html;base64,PHNjcmlwdD4=\")",
        ] {
            for decl in [
                format!("background: {u};"),
                format!("background: red {u} no-repeat;"),
                format!("cursor: {u}, auto;"),
                format!("background: linear-gradient(red, blue), {u};"),
            ] {
                let r = avec(&format!(".a {{ {decl} }}\n.b {{ color: red }}"));
                assert!(r.erreurs.is_empty(), "{decl} : {:?}", r.erreurs);
                assert!(sans_url(&r), "{decl} : {}", r.css_assaini);
                assert!(!r.css_assaini.contains("evil"), "{decl}");
                assert!(r.avertissements.iter().any(|a| a.contains("url()")), "{decl}");
                assert!(r.css_assaini.contains(".b { color: red }"), "{decl}");
            }
        }
    }

    #[test]
    fn data_svg_refuse() {
        for u in [
            "url(\"data:image/svg+xml;base64,PHN2Zy8+\")",
            "url('data:image/svg+xml;base64,PHN2Zy8+')",
            "url(data:image/svg+xml;base64,PHN2Zy8+)",
            "url(\"data:image/svg+xml;utf8,<svg xmlns='http://www.w3.org/2000/svg'/>\")",
            "url(\"data:image/svg;base64,PHN2Zy8+\")",
            "url(\"data:image/png;charset=utf-8,abc\")",
            "url(\"data:image/png,abc\")",
            "url(\"data:image/jpg;base64,AAAA\")",
            "url(\"DATA:IMAGE/SVG+XML;base64,AAAA\")",
        ] {
            let r = avec(&format!(".a {{ background: {u}; }}"));
            assert!(r.erreurs.is_empty(), "{u}");
            assert!(sans_url(&r), "{u} : {}", r.css_assaini);
            assert!(!r.avertissements.is_empty(), "{u}");
        }
    }

    #[test]
    fn data_image_admise() {
        for (u, garde) in [
            ("url(\"data:image/png;base64,iVBORw0KGgo=\")", true),
            ("url('data:image/jpeg;base64,/9j/4AAQ')", true),
            ("url(data:image/webp;base64,UklGRg==)", true),
            ("url(\"data:image/gif;base64,R0lGODlh\")", true),
            ("url( \"data:image/avif;base64,AAAA\" )", true),
            ("url(\"DATA:IMAGE/PNG;BASE64,iVBORw0KGgo=\")", true),
            // Charge utile avec un caractère hors base64 : supprimée.
            ("url(\"data:image/png;base64,iVBOR w0KGgo=\")", false),
            ("url(\"data:image/png;base64,iVBOR\\41w0KGgo=\")", false),
            ("url(\"data:image/png;base64,AAAA\" extra)", false),
        ] {
            let r = avec(&format!(".a {{ background: {u}; }}"));
            assert!(r.erreurs.is_empty(), "{u} : {:?}", r.erreurs);
            assert_eq!(!sans_url(&r), garde, "{u} : {}", r.css_assaini);
        }
        // Avec un point-virgule dans l'adresse non guillemetée : l'unité n'est pas coupée.
        let r = avec(".a { background: url(data:image/png;base64,AAAA); color: red; }");
        assert!(r.css_assaini.contains("url(data:image/png;base64,AAAA)"));
        assert!(r.css_assaini.contains("color: red;"));
    }

    #[test]
    fn data_image_trop_grosse() {
        let ok = "A".repeat(TAILLE_MAX_DATA_URL);
        let trop = "A".repeat(TAILLE_MAX_DATA_URL + 4);
        let r = avec(&format!(".a {{ background: url(\"data:image/png;base64,{ok}\"); }}"));
        assert!(!sans_url(&r), "à la limite : acceptée");
        let r = avec(&format!(".a {{ background: url(\"data:image/png;base64,{trop}\"); }}"));
        assert!(sans_url(&r));
        assert!(r.erreurs.is_empty());
    }

    #[test]
    fn data_image_et_url_externe_dans_la_meme_declaration() {
        let r = avec(".a { background: url(\"data:image/png;base64,AAAA\"), url(http://evil.example/x.png); }");
        assert!(sans_url(&r), "{}", r.css_assaini);
    }

    #[test]
    fn obfuscation_par_echappement_rejetee() {
        for css in [
            ".a { background: \\75rl(http://evil.example/x.png); }",
            ".a { background: \\75 rl(http://evil.example/x.png); }",
            ".a { background: u\\72l(http://evil.example/x.png); }",
            ".a { background: ur\\6c(http://evil.example/x.png); }",
            ".a { background: \\000075rl(http://evil.example/x.png); }",
            ".a { background: \\55RL(http://evil.example/x.png); }",
            ".a { background: ur\\\nl(http://evil.example/x.png); }",
            ".a { background: url\\(http://evil.example/x.png); }",
            "@\\69mport 'x.css';",
            "@imp\\6frt 'x.css';",
            "@\\66ont-face { src: local(x) }",
            ".a { b\\65havior: x }",
            ".a { width: \\65xpression(1) }",
            ".a { background: im\\61ge-set('a.png' 1x) }",
            ".a { background: url(x.png) \\75rl(y.png); }",
        ] {
            let r = valider_css(css);
            assert!(!r.erreurs.is_empty(), "devait être rejeté : {css}");
            assert!(r.css_assaini.is_empty(), "{css}");
        }
    }

    #[test]
    fn obfuscation_par_commentaire_supprimee_ou_rejetee_jamais_conservee() {
        for css in [
            ".a { background: ur/**/l(http://evil.example/x.png); }",
            ".a { background: u/* x */r/* y */l(http://evil.example/x.png); }",
            ".a { background: url/**/(http://evil.example/x.png); }",
            "@imp/**/ort 'x.css';",
            "@font/**/-face { src: local(x) }",
            ".a { beha/**/vior: x }",
            ".a { width: expres/**/sion(1) }",
        ] {
            let r = valider_css(css);
            assert!(r.erreurs.is_empty(), "{css} : {:?}", r.erreurs);
            let bas = r.css_assaini.to_ascii_lowercase();
            for interdit in ["url(", "@import", "@font-face", "behavior", "expression(", "evil"] {
                assert!(!bas.contains(interdit), "{css} -> {}", r.css_assaini);
            }
            assert!(!r.avertissements.is_empty(), "{css}");
        }
    }

    #[test]
    fn commentaire_etranger_ne_masque_pas_une_url() {
        // Une url( dans un commentaire n'est qu'un commentaire : conservée, inoffensive.
        let r = avec("/* url(http://evil.example) */ .a { color: red; }");
        assert!(r.erreurs.is_empty());
        assert!(r.css_assaini.contains(".a { color: red; }"));
        // Mais une url réelle à côté du commentaire est supprimée.
        let r = avec("/* ok */ .a { background: url(http://evil.example/x); }");
        assert!(r.css_assaini.contains("/* ok */"));
        assert!(!r.css_assaini.contains("background"));
    }

    #[test]
    fn url_dans_une_chaine_est_supprimee_par_prudence() {
        let r = avec(".a { content: \"url(x)\"; color: red; }\n.b { color: blue }");
        assert!(sans_url(&r));
        assert!(r.css_assaini.contains(".b { color: blue }"));
    }

    #[test]
    fn echappements_legitimes_conserves() {
        let r = avec(".a::before { content: \"\\201C\"; }\n.b\\:c { color: red; }\n.d { content: \"a\\;b\"; }");
        assert!(r.erreurs.is_empty(), "{:?}", r.erreurs);
        assert!(r.css_assaini.contains("content: \"\\201C\";"));
        assert!(r.css_assaini.contains(".b\\:c { color: red; }"));
        assert!(r.css_assaini.contains("content: \"a\\;b\";"));
        assert!(r.avertissements.is_empty(), "{:?}", r.avertissements);
    }

    #[test]
    fn structures_ambigues_rejetees() {
        for css in ["/* jamais fermé .a{}", ".a { content: \"jamais fermée }", ".a { background: foo(bar; }"] {
            let r = valider_css(css);
            assert!(!r.erreurs.is_empty(), "{css}");
            assert!(r.css_assaini.is_empty());
        }
    }

    #[test]
    fn nom_et_contrat_conserves_meme_si_la_premiere_regle_saute() {
        let r = valider_css("/* @nom: Mon thème */\n/* @contrat: 2 */\n@import 'x.css';\n:root { color: red }");
        assert!(r.css_assaini.contains("/* @nom: Mon thème */"));
        assert!(r.css_assaini.contains("/* @contrat: 2 */"));
        assert!(!r.css_assaini.contains("@import"));
    }

    #[test]
    fn idempotent() {
        let entree = format!(
            "/* @nom: T */\n@import 'x';\n{}.a {{ background: url(x); color: red; }}\n.b {{ background: url(\"data:image/png;base64,AAAA\") }}",
            base()
        );
        let r1 = valider_css(&entree);
        assert!(r1.erreurs.is_empty());
        let r2 = valider_css(&r1.css_assaini);
        assert_eq!(r1.css_assaini, r2.css_assaini);
        assert!(r2.avertissements.is_empty(), "{:?}", r2.avertissements);
    }

    #[test]
    fn numeros_de_ligne() {
        let r = valider_css("/* a */\n\n.a { color: red; }\n.b {\n  background: url(x);\n}\n");
        assert!(r.avertissements.iter().any(|a| a.starts_with("ligne 5 : url()")), "{:?}", r.avertissements);
    }

    #[test]
    fn avertissements_de_contenu() {
        let r = valider_css(":root { --cr-fond: ; --cr-accent: red; --cr-bidule: 1; }");
        assert!(r.erreurs.is_empty());
        let a = r.avertissements.join("\n");
        assert!(a.contains("variable obligatoire absente : --cr-texte "), "{a}");
        assert!(a.contains("valeur vide : --cr-fond"), "{a}");
        assert!(a.contains("variable inconnue : --cr-bidule"), "{a}");
        assert!(a.contains("color-scheme absent"), "{a}");
        assert!(!a.contains("--cr-accent "), "{a}");
        // Facultatives absentes : aucun avertissement.
        assert!(!a.contains("--cr-succes"), "{a}");
    }

    #[test]
    fn variables_cr_et_valeur_effective() {
        let v =
            variables_cr(":root { --cr-fond: #fff; --autre: 1; --cr-fond: #000 !important; }\n.x { --cr-accent: red }");
        assert_eq!(v.len(), 3);
        assert_eq!(valeur_variable(&v, "--cr-fond"), Some("#000 !important"));
        assert_eq!(valeur_variable(&v, "--cr-accent"), Some("red"));
        assert_eq!(valeur_variable(&v, "--cr-texte"), None);
    }
}

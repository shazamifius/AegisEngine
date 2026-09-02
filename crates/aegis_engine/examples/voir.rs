//! # VOIR — la planche des images de preuve, qui ne se recopie plus à la main
//!
//! ```text
//! cargo run --release -p aegis_engine --example voir -- <dossier> [sortie.html]
//! ```
//!
//! ## ⚠ Le défaut qu'il corrige, et c'est un défaut connu du projet
//!
//! La page existait, et **sa liste d'images était écrite à la main dans le HTML**. Elle en montrait
//! trois pendant que le dossier en portait dix-sept : les quatorze autres n'existaient pour
//! personne. Ajouter une image demandait de rouvrir la page et d'y coller un bloc — donc, tôt ou
//! tard, de l'oublier.
//!
//! *C'est mot pour mot la leçon du 22 août 2026 : « un texte se recopie, donc diverge ; une
//! commande, non ».* La correction n'était pas d'écrire une meilleure page : c'est de la
//! **calculer**. Ce programme liste le dossier, lit les dimensions dans l'en-tête de chaque PNG,
//! trie du plus récent au plus ancien, et rend la planche. Une image qui apparaît dans le dossier
//! apparaît sur la planche, sans que personne ait rien à faire.
//!
//! ## Ce qu'il n'est pas
//!
//! Pas un serveur, pas une dépendance, pas un autre langage : un programme Rust qui écrit un
//! fichier. La page, elle, recharge ses images toutes les deux secondes — donc **une image
//! regénérée pendant qu'on travaille se met à jour sous les yeux**, sans toucher à l'onglet.
//!
//! ⚠ Le HTML est un *médium*, pas du code embarqué : rien ici n'entre dans le moteur, qui reste
//! entièrement Rust.

use std::fs;
use std::path::Path;
use std::time::UNIX_EPOCH;

/// Les dimensions d'un PNG, lues dans son en-tête IHDR.
///
/// La signature fait 8 octets, puis le premier bloc est toujours IHDR : longueur (4), type (4),
/// puis largeur et hauteur sur 4 octets chacune, **en gros-boutiste**. C'est fixé par le format,
/// donc ça ne demande ni décodeur ni bibliothèque.
fn dimensions(octets: &[u8]) -> Option<(u32, u32)> {
    if octets.len() < 24 || &octets[..8] != b"\x89PNG\r\n\x1a\n" {
        return None;
    }
    let lire = |d: usize| u32::from_be_bytes([octets[d], octets[d + 1], octets[d + 2], octets[d + 3]]);
    Some((lire(16), lire(20)))
}

/// L'heure d'un fichier, en « HH:MM:SS » locale.
///
/// ⚠ Sans dépendance : les secondes depuis l'époque, ramenées dans la journée. Le décalage horaire
/// est lu dans `TZ`… qui n'est presque jamais posée. **On affiche donc UTC**, et on le DIT — un
/// horaire faux de deux heures qu'on croit local est pire qu'un horaire annoncé UTC.
fn heure_utc(secondes: u64) -> String {
    let jour = secondes % 86_400;
    format!("{:02}:{:02}:{:02} UTC", jour / 3600, (jour % 3600) / 60, jour % 60)
}

/// Une image de preuve : son nom, sa date, sa taille, ses dimensions.
///
/// ⚠ Nommée plutôt que laissée en quadruplet anonyme — clippy le demandait, et il avait raison :
/// `(String, u64, u64, Option<(u32, u32)>)` ne dit pas lequel des deux `u64` est la date.
struct Preuve {
    nom: String,
    modifie: u64,
    taille: u64,
    dims: Option<(u32, u32)>,
}

fn main() {
    let mut args = std::env::args().skip(1);
    let dossier = args.next().unwrap_or_else(|| {
        eprintln!("usage : voir <dossier> [sortie.html]");
        std::process::exit(2);
    });
    let sortie = args.next().unwrap_or_else(|| "/tmp/aegis-voir.html".into());

    let chemin = Path::new(&dossier);
    let mut images: Vec<Preuve> = Vec::new();
    let entrees = fs::read_dir(chemin).unwrap_or_else(|e| {
        eprintln!("{dossier} : {e}");
        std::process::exit(1);
    });
    for entree in entrees.flatten() {
        let nom = entree.file_name().to_string_lossy().into_owned();
        if !nom.to_lowercase().ends_with(".png") {
            continue;
        }
        let Ok(meta) = entree.metadata() else { continue };
        let modifie = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);
        // Seul l'en-tête sert : on lit le fichier entier parce qu'il fait quelques dizaines de Ko
        // et que la simplicité vaut plus ici que l'économie.
        let taille = meta.len();
        let dims = fs::read(entree.path()).ok().and_then(|o| dimensions(&o));
        images.push(Preuve { nom, modifie, taille, dims });
    }

    if images.is_empty() {
        eprintln!("aucun PNG dans {dossier}");
        std::process::exit(1);
    }
    // Le plus récent d'abord : ce qu'on vient de produire est ce qu'on veut voir.
    images.sort_by(|a, b| b.modifie.cmp(&a.modifie).then(a.nom.cmp(&b.nom)));

    let absolu = fs::canonicalize(chemin).unwrap_or_else(|_| chemin.to_path_buf());
    let mut figures = String::new();
    for Preuve { nom, modifie, taille, dims } in &images {
        let taille_lisible = if *taille >= 1024 {
            format!("{} Ko", taille / 1024)
        } else {
            format!("{taille} o")
        };
        let mesure = match dims {
            Some((l, h)) => format!("{l}×{h} · "),
            None => String::new(),
        };
        figures.push_str(&format!(
            "<figure><div class=\"cadre\"><img src=\"file://{}/{nom}\" alt=\"{nom}\" loading=\"lazy\"></div>\
             <figcaption><b>{nom}</b><br><span>{mesure}{taille_lisible} · {}</span></figcaption></figure>\n",
            absolu.display(),
            heure_utc(*modifie)
        ));
    }

    let page = format!(
        r#"<!doctype html><html lang="fr"><head><meta charset="utf-8">
<title>Aegis — ce que je vois</title>
<style>
  :root {{ color-scheme: dark; }}
  body {{ margin:0; padding:28px 32px 60px; background:#14151a; color:#c9ccd4;
         font:14px/1.6 system-ui,-apple-system,"Segoe UI",sans-serif; }}
  h1 {{ font-size:15px; font-weight:600; letter-spacing:.06em; text-transform:uppercase;
       color:#7e8492; margin:0 0 4px; }}
  .sous {{ color:#5f6472; font-size:12.5px; margin:0 0 26px; }}
  .grille {{ display:grid; gap:26px; align-items:start;
            grid-template-columns:repeat(auto-fill,minmax(300px,1fr)); }}
  figure {{ margin:0; background:#1c1e25; border:1px solid #272a33; border-radius:10px;
           padding:12px; }}
  /* Le damier dit ou l'image est transparente, sans mentir sur ses couleurs. */
  .cadre {{ background:
      linear-gradient(45deg,#22242c 25%,transparent 25%,transparent 75%,#22242c 75%) 0 0/18px 18px,
      linear-gradient(45deg,#22242c 25%,#1a1c22 25%,#1a1c22 75%,#22242c 75%) 9px 9px/18px 18px;
      border-radius:6px; overflow:hidden; line-height:0; }}
  img {{ display:block; width:100%; height:auto; }}
  figcaption {{ margin-top:10px; font-size:12.5px; color:#9aa0ad; word-break:break-all; }}
  figcaption b {{ color:#dfe3ea; font-weight:600; }}
  figcaption span {{ color:#5f6472; }}
  .pied {{ position:fixed; left:0; right:0; bottom:0; padding:7px 32px; background:#0f1015;
          border-top:1px solid #272a33; color:#5f6472; font-size:12px; }}
  .pastille {{ display:inline-block; width:7px; height:7px; border-radius:50%; background:#4ea87a;
              margin-right:7px; vertical-align:middle; }}
</style></head><body>
<h1>Aegis — ce que je vois</h1>
<p class="sous">{} image(s) dans <code>{}</code>, les plus récentes d'abord.
   Cette planche est <b>calculée</b>, jamais recopiée : une image qui apparaît dans le dossier
   apparaît ici. Laisse l'onglet ouvert.</p>
<div class="grille">
{figures}</div>
<div class="pied"><span class="pastille"></span>en veille sur les fichiers — une image regénérée apparaît en 2 s</div>
<script>
// Recharge UNIQUEMENT les images, jamais la page : la position de lecture ne bouge pas, et
// l'onglet peut rester ouvert des heures pendant qu'on travaille.
setInterval(() => {{
  for (const img of document.images) {{
    const base = img.src.split('?')[0];
    img.src = base + '?t=' + Date.now();
  }}
}}, 2000);
</script>
</body></html>"#,
        images.len(),
        absolu.display()
    );

    fs::write(&sortie, page).unwrap_or_else(|e| {
        eprintln!("{sortie} : {e}");
        std::process::exit(1);
    });
    println!("{} image(s) → {sortie}", images.len());
    for Preuve { nom, dims, .. } in images.iter().take(4) {
        match dims {
            Some((l, h)) => println!("  {nom}  {l}×{h}"),
            None => println!("  {nom}  (en-tête PNG illisible)"),
        }
    }
    if images.len() > 4 {
        println!("  … et {} autres", images.len() - 4);
    }
}

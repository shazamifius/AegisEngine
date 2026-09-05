//! **LA CONNECTIVITÉ EST-ELLE VRAIMENT GRATUITE ? — le banc qui répond, sur du vrai contenu.**
//!
//! ```text
//! cargo run --release -p aegis_engine --example topologie --no-default-features
//! cargo run --release -p aegis_engine --example topologie --no-default-features -- <fichier.glb>
//! ```
//!
//! ## Pourquoi ce banc existe (5 septembre 2026)
//!
//! Le choix de socle `0.a` demande où la lumière indirecte est stockée. Deux familles s'opposent :
//! un **nuage de surfels** (des disques semés sur la surface, sans topologie) ou un **maillage de
//! micro-triangles barycentriques** (la surface elle-même, subdivisée).
//!
//! L'argument n° 1 en faveur du micro-triangle est que **sa connectivité est gratuite** : deux
//! triangles qui se touchent le savent déjà, là où un nuage de surfels doit construire une table de
//! hachage pour retrouver ses voisins. Quatre systèmes publiés paient ce rattrapage — table de
//! hachage à trois passes, hiérarchie de voxels, 32 octets de pointeurs par entrée, ou des trous
//! entre surfels à reboucher.
//!
//! ⚠ **Mais cet argument suppose un maillage propre, et rien ne le garantit.** Le cap du projet est
//! le contenu créé par des joueurs : maillages non-manifold, sommets dupliqués, triangles
//! dégénérés. **Une adjacence gratuite sur un cas idéal n'est pas une adjacence gratuite.**
//!
//! ## ⚠⚠ LE PIÈGE QUE CE BANC ÉVITE, ET IL AURAIT MENTI DE FAÇON PLAUSIBLE
//!
//! Un exportateur duplique les sommets aux coutures : deux triangles qui se touchent dans l'espace
//! peuvent porter des **indices différents**, parce qu'ils ne partagent ni la normale ni l'UV. Une
//! sonde qui ne regarde que les indices verrait alors une surface en miettes et conclurait que la
//! connectivité n'existe pas — alors que la surface est parfaitement connexe. *Mesuré sur le modèle
//! de test : la lecture brute ne voit que 37 % de l'adjacence réelle.*
//!
//! **Ce banc mesure donc DEUX topologies** : celle des indices bruts (ce que le fichier donne) et
//! celle des positions soudées (ce que la surface EST). *L'écart entre les deux est le vrai
//! résultat : il dit ce que coûterait une soudure au chargement.*
//!
//! ⭐ **Et la soudure ne coûte aucune constante arbitraire** : elle compare les positions
//! **bit à bit**. Un sommet dupliqué pour cause de couture porte la position **recopiée telle
//! quelle** — il n'y a rien à tolérer. *Une tolérance aurait été un chiffre de plus à justifier
//! pour toujours ; ici la question ne se pose pas.* (Seul `-0.0` est ramené à `+0.0`, parce que
//! deux zéros de signes opposés sont le même point.)
//!
//! ## ⭐ Ce banc lit par le VRAI chargeur du moteur, et c'est délibéré
//!
//! Sa première version portait son propre parseur GLB, parce que `GlbLoader` ne lisait alors que
//! `meshes[0].primitives[0]` — un tiers du modèle de test, en silence. **C'est ce banc qui a révélé
//! la dette**, et elle a été levée le jour même par [`GlbLoader::charger_scene`].
//!
//! Il passe donc désormais par le moteur. *Un banc qui garde son propre décodeur finit par mesurer
//! son décodeur : les deux divergent, et c'est toujours celui qu'on ne relit plus qui se met à
//! mentir.*
//!
//! ## Ce que ce banc ne prouve PAS
//!
//! - Il ne dit rien du COÛT en temps du parcours de rayons : il mesure une structure, pas une
//!   vitesse.
//! - Un seul modèle ne fait pas un corpus, et un modèle **propre** encore moins : une topologie
//!   régulière est le meilleur cas possible pour l'adjacence. Le chiffre obtenu est un **plafond**.
//! - Les aires sont exactes ; le nombre de micro-triangles qu'on en déduit dépend d'une densité
//!   cible, qui est un choix. Le banc en balaie donc plusieurs plutôt que d'en figer une.

use aegis_engine::geometry::glb_loader::{GlbLoader, Scene};
use std::collections::HashMap;
use std::path::PathBuf;

/// Le modèle par défaut : celui qu'il a préparé pour cette mesure.
const MODELE_PAR_DEFAUT: &str = "assets/modeles/table de teste verre.glb";

/// Le verdict topologique d'un maillage : chaque arête est portée par 1, 2, ou ≥3 triangles.
#[derive(Default, Clone, Copy)]
struct Topologie {
    /// Une seule face : c'est un **bord**. Légitime — un plan en a, un cylindre ouvert aussi.
    bord: usize,
    /// Deux faces : **2-manifold**. C'est l'adjacence exploitable.
    manifold: usize,
    /// Trois faces ou plus : **non-manifold**. L'adjacence y est ambiguë.
    non_manifold: usize,
}

impl Topologie {
    /// La part d'arêtes qui offrent une adjacence utilisable, **bords exclus du dénominateur** :
    /// un bord n'est pas un défaut du maillage, c'est une propriété de la forme. Ce qui menace
    /// l'argument « connectivité gratuite », c'est le non-manifold.
    fn part_exploitable(&self) -> f64 {
        let interieur = self.manifold + self.non_manifold;
        if interieur == 0 {
            return 1.0;
        }
        self.manifold as f64 / interieur as f64
    }

    fn cumuler(&mut self, autre: &Topologie) {
        self.bord += autre.bord;
        self.manifold += autre.manifold;
        self.non_manifold += autre.non_manifold;
    }
}

/// Compte les arêtes selon le nombre de faces qui les portent.
///
/// Une arête est une paire **non ordonnée** de sommets : `(min, max)`. Deux triangles adjacents la
/// parcourent en sens inverse, et c'est cela qui les rend voisins.
fn topologie(indices: &[u32], remap: &dyn Fn(u32) -> u32) -> Topologie {
    let mut faces_par_arete: HashMap<(u32, u32), usize> = HashMap::new();

    for t in indices.chunks_exact(3) {
        let (a, b, c) = (remap(t[0]), remap(t[1]), remap(t[2]));
        for (u, v) in [(a, b), (b, c), (c, a)] {
            // Une arête dont les deux bouts sont le même sommet appartient à un triangle
            // dégénéré : elle n'est pas une adjacence, elle est comptée à part.
            if u == v {
                continue;
            }
            let cle = if u < v { (u, v) } else { (v, u) };
            *faces_par_arete.entry(cle).or_insert(0) += 1;
        }
    }

    let mut t = Topologie::default();
    for n in faces_par_arete.values() {
        match n {
            1 => t.bord += 1,
            2 => t.manifold += 1,
            _ => t.non_manifold += 1,
        }
    }
    t
}

/// Construit la table qui envoie chaque indice de sommet vers un représentant unique **par
/// position**. C'est ce qui recolle les sommets qu'un exportateur a dupliqués aux coutures.
///
/// La comparaison est **bit à bit** : aucune tolérance, donc aucune constante à justifier.
fn souder(positions: &[[f32; 3]]) -> Vec<u32> {
    let mut representant: HashMap<[u32; 3], u32> = HashMap::new();
    let mut table = Vec::with_capacity(positions.len());

    for (i, p) in positions.iter().enumerate() {
        // `-0.0` et `+0.0` sont le même point de l'espace, et n'ont pas les mêmes bits.
        let bits = [
            (if p[0] == 0.0 { 0.0 } else { p[0] }).to_bits(),
            (if p[1] == 0.0 { 0.0 } else { p[1] }).to_bits(),
            (if p[2] == 0.0 { 0.0 } else { p[2] }).to_bits(),
        ];
        let r = *representant.entry(bits).or_insert(i as u32);
        table.push(r);
    }
    table
}

/// L'aire d'un triangle : la moitié de la norme du produit vectoriel de deux de ses côtés.
fn aire(a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> f64 {
    let u = [(b[0] - a[0]) as f64, (b[1] - a[1]) as f64, (b[2] - a[2]) as f64];
    let v = [(c[0] - a[0]) as f64, (c[1] - a[1]) as f64, (c[2] - a[2]) as f64];
    let n = [
        u[1] * v[2] - u[2] * v[1],
        u[2] * v[0] - u[0] * v[2],
        u[0] * v[1] - u[1] * v[0],
    ];
    0.5 * (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt()
}

/// Le niveau de subdivision qui amène un triangle d'aire `a` à une aire cible `cible`.
///
/// $k(T) = \lceil \log_2 \sqrt{A(T)/A^*} \rceil$, borné à 0 : un triangle déjà plus petit que la
/// cible ne se subdivise pas. Il porte alors `4^k` micro-triangles.
fn niveau_subdivision(a: f64, cible: f64) -> u32 {
    if a <= cible || cible <= 0.0 {
        return 0;
    }
    (a / cible).sqrt().log2().ceil().max(0.0) as u32
}

/// Les aires, en espace monde, de tous les triangles d'une scène.
fn aires_de_la_scene(scene: &Scene) -> Vec<f64> {
    scene
        .indices
        .chunks_exact(3)
        .map(|t| {
            aire(
                scene.sommets[t[0] as usize].position,
                scene.sommets[t[1] as usize].position,
                scene.sommets[t[2] as usize].position,
            )
        })
        .collect()
}

fn main() {
    let chemin = match std::env::args().nth(1) {
        Some(a) => PathBuf::from(a),
        None => racine_du_depot().join(MODELE_PAR_DEFAUT),
    };

    titre("AEGIS — LA CONNECTIVITÉ EST-ELLE GRATUITE ?");
    println!("  Fichier : {}", chemin.display());
    println!("  Lu par `GlbLoader::charger_scene` — donc exactement ce que le moteur voit.\n");

    let scene = match GlbLoader::charger_scene(&chemin) {
        Ok(s) => s,
        Err(e) => {
            println!("  Lecture impossible : {e}");
            return;
        }
    };

    // ── Ce que le fichier contient ──────────────────────────────────────────
    titre("CE QUE LE FICHIER CONTIENT");
    println!("  {:<18} {:>9} {:>10} {:>14}", "partie", "sommets", "triangles", "aire monde");
    let aires_tous = aires_de_la_scene(&scene);
    let mut aire_totale = 0.0f64;
    for p in &scene.parties {
        let debut = (p.premier_indice / 3) as usize;
        let fin = debut + (p.nombre_indices / 3) as usize;
        let a: f64 = aires_tous[debut..fin].iter().sum();
        aire_totale += a;
        println!(
            "  {:<18} {:>9} {:>10} {:>13.3}",
            tronquer(&p.nom, 18),
            p.nombre_sommets,
            p.nombre_indices / 3,
            a
        );
    }
    let triangles_total = scene.indices.len() / 3;
    println!(
        "  {:<18} {:>9} {:>10} {:>13.3}",
        "TOTAL",
        scene.sommets.len(),
        triangles_total,
        aire_totale
    );

    // ── La topologie, dans les deux lectures ────────────────────────────────
    titre("LA TOPOLOGIE — indices bruts, puis positions soudées");
    println!("  Une arête portée par 2 faces offre une adjacence. Par 1, c'est un bord (légitime).");
    println!("  Par 3 ou plus, elle est non-manifold : l'adjacence y est ambiguë.");
    println!("  ⚠ Chaque partie est soudée SÉPARÉMENT : deux objets qui se touchent ne sont pas");
    println!("    la même surface, et les confondre inventerait une adjacence.\n");
    println!("  {:<18} {:>26} {:>26}", "", "── indices bruts ──", "── positions soudées ──");
    println!(
        "  {:<18} {:>8} {:>8} {:>8} {:>8} {:>8} {:>8}",
        "partie", "bord", "adjac.", "non-mfd", "bord", "adjac.", "non-mfd"
    );

    let mut brut_total = Topologie::default();
    let mut soude_total = Topologie::default();
    let mut sommets_soudes_total = 0usize;

    for p in &scene.parties {
        let i0 = p.premier_indice as usize;
        let i1 = i0 + p.nombre_indices as usize;
        let s0 = p.premier_sommet as usize;
        // Les indices de la partie, ramenés à sa propre plage de sommets.
        let locaux: Vec<u32> = scene.indices[i0..i1].iter().map(|i| i - p.premier_sommet).collect();
        let positions: Vec<[f32; 3]> = scene.sommets[s0..s0 + p.nombre_sommets as usize]
            .iter()
            .map(|v| v.position)
            .collect();

        let brut = topologie(&locaux, &|i| i);
        let table = souder(&positions);
        let soude = topologie(&locaux, &|i| table[i as usize]);

        let uniques = {
            let mut v = table.clone();
            v.sort_unstable();
            v.dedup();
            v.len()
        };
        sommets_soudes_total += uniques;
        brut_total.cumuler(&brut);
        soude_total.cumuler(&soude);

        println!(
            "  {:<18} {:>8} {:>8} {:>8} {:>8} {:>8} {:>8}",
            tronquer(&p.nom, 18),
            brut.bord, brut.manifold, brut.non_manifold,
            soude.bord, soude.manifold, soude.non_manifold
        );
    }
    println!(
        "  {:<18} {:>8} {:>8} {:>8} {:>8} {:>8} {:>8}",
        "TOTAL",
        brut_total.bord, brut_total.manifold, brut_total.non_manifold,
        soude_total.bord, soude_total.manifold, soude_total.non_manifold
    );

    let sommets_total = scene.sommets.len();
    println!(
        "\n  Sommets : {sommets_total} dans le fichier → {sommets_soudes_total} positions distinctes \
         ({:.1} % de duplication)",
        100.0 * (sommets_total - sommets_soudes_total) as f64 / sommets_total.max(1) as f64
    );
    println!(
        "  Adjacence exploitable (hors bords) : {:.2} % bruts  →  {:.2} % soudés",
        100.0 * brut_total.part_exploitable(),
        100.0 * soude_total.part_exploitable()
    );
    println!(
        "  Arêtes intérieures réellement adjacentes : {} brutes → {} soudées  ({:.0} % vues sans soudure)",
        brut_total.manifold,
        soude_total.manifold,
        100.0 * brut_total.manifold as f64 / soude_total.manifold.max(1) as f64
    );

    let degeneres = aires_tous.iter().filter(|a| **a == 0.0).count();
    println!(
        "  Triangles d'aire nulle : {degeneres} sur {triangles_total} ({:.3} %)",
        100.0 * degeneres as f64 / triangles_total.max(1) as f64
    );

    // ── La dispersion des aires : l'objection n° 1 contre le triangle-support ─
    titre("LA DISPERSION DES AIRES — l'objection n° 1, chiffrée");
    println!("  « La taille d'un triangle varie de mille à un dans une scène réelle » — vérifions.\n");
    let mut aires: Vec<f64> = aires_tous.iter().copied().filter(|a| *a > 0.0).collect();
    aires.sort_by(|a, b| a.partial_cmp(b).unwrap());

    let q = |f: f64| aires[((aires.len() - 1) as f64 * f) as usize];
    println!("  minimum   {:.3e}", aires[0]);
    println!("  médiane   {:.3e}", q(0.5));
    println!("  99 %      {:.3e}", q(0.99));
    println!("  maximum   {:.3e}", aires[aires.len() - 1]);
    println!("  rapport max/min        : {:.0} ×", aires[aires.len() - 1] / aires[0]);
    println!("  rapport 99 % / médiane : {:.1} ×", q(0.99) / q(0.5));

    // ── Ce que ça coûterait, dans les deux familles ─────────────────────────
    titre("CE QUE CHAQUE FAMILLE COÛTERAIT SUR CE MODÈLE");
    println!("  La densité cible n'est pas de moi : `On-Surface Caches` (HPG 2024) tient ses caches");
    println!("  secondaires entre 20 et 30 cm de côté. On balaie autour, plus fin et plus grossier.\n");
    println!("  Un micro-triangle ne stocke QUE sa radiance (6 o) : sa position et sa normale se");
    println!("  recalculent depuis (u,v) et son triangle parent. Un surfel doit porter les siennes");
    println!("  (16 o : position, normale octaédrique, rayon, radiance).\n");
    println!(
        "  {:>10} {:>14} {:>13} {:>11} {:>11} {:>9}",
        "côté", "µ-triangles", "surfels", "µ-tri Mo", "surfel Mo", "gain"
    );

    for cote in [0.01f64, 0.05, 0.20, 0.30] {
        let cible = cote * cote;
        let micro: u64 = aires
            .iter()
            .map(|a| 4u64.saturating_pow(niveau_subdivision(*a, cible)))
            .sum();
        let surfels = (aire_totale / cible).ceil() as u64;
        let mo_micro = micro as f64 * 6.0 / 1_048_576.0;
        let mo_surfel = surfels as f64 * 16.0 / 1_048_576.0;
        println!(
            "  {:>8.0} cm {:>14} {:>13} {:>11.2} {:>11.2} {:>8.2}×",
            cote * 100.0, micro, surfels, mo_micro, mo_surfel,
            mo_surfel / mo_micro.max(f64::MIN_POSITIVE)
        );
    }

    println!("\n  ⚠ Le nombre de µ-triangles est ≥ celui des surfels, pour DEUX raisons structurelles :");
    println!("     · un micro-maillage ne peut pas avoir moins d'échantillons que le maillage a de");
    println!("       triangles — c'est un PLANCHER, et un surfel n'en a pas ;");
    println!("     · la subdivision ne progresse que par puissances de 4, donc elle DÉPASSE la");
    println!("       densité cible au lieu de l'atteindre.");
    println!("     La colonne « gain » dit si les 6 octets contre 16 couvrent ce surcoût.");

    titre("CE QUE CE BANC NE DIT PAS");
    println!("  · Rien sur le TEMPS : il mesure une structure, pas une vitesse.");
    println!("  · Rien sur un autre modèle — et un modèle PROPRE est le meilleur cas possible pour");
    println!("    l'adjacence : le chiffre obtenu est un plafond, pas une moyenne.");
    println!("  · Rien sur le contenu d'un joueur inconnu — c'est justement ce qui reste ouvert.");
    println!("  · Rien en radiance DIRECTIONNELLE : le rapport 6/16 suppose une radiance scalaire.");
    println!("    `On-Surface Caches` mesure 1134 o par entrée avec un hémisphère 8×8 — et alors");
    println!("    les deux familles paient le même hémisphère, donc l'avantage mémoire s'évapore.");
}

fn tronquer(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        s.chars().take(n - 1).collect::<String>() + "…"
    }
}

fn titre(t: &str) {
    println!("\n\x1b[1m{t}\x1b[0m");
    println!("{}", "─".repeat(t.chars().count()));
}

fn racine_du_depot() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    while !p.join("assets").is_dir() {
        if !p.pop() {
            return PathBuf::from(".");
        }
    }
    p
}

// ─────────────────────────────────────────────────────────────────────────────
// Les gardes — chaque mesure confrontée à une vérité analytique, jamais à elle-même
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Deux triangles formant un carré partagent leur diagonale : **1 arête adjacente, 4 bords**.
    /// C'est la plus petite topologie dont la réponse se compte à la main.
    #[test]
    fn un_carre_de_deux_triangles_a_une_seule_arete_adjacente() {
        let indices = [0, 1, 2, 0, 2, 3];
        let t = topologie(&indices, &|i| i);
        assert_eq!(t.manifold, 1, "la diagonale est la seule arête partagée");
        assert_eq!(t.bord, 4, "les quatre côtés du carré sont des bords");
        assert_eq!(t.non_manifold, 0);
        assert_eq!(t.bord + t.manifold + t.non_manifold, 5, "un carré a cinq arêtes en tout");
    }

    /// Trois triangles sur la même arête : c'est le cas non-manifold, et il doit être vu.
    #[test]
    fn trois_faces_sur_une_arete_sont_vues_comme_non_manifold() {
        let indices = [0, 1, 2, 0, 1, 3, 0, 1, 4];
        let t = topologie(&indices, &|i| i);
        assert_eq!(t.non_manifold, 1, "l'arête 0-1 porte trois faces");
        assert_eq!(t.manifold, 0);
    }

    /// ⭐ LA GARDE QUI COMPTE : un carré dont les sommets de la diagonale sont DUPLIQUÉS — ce que
    /// fait tout exportateur à une couture. Les indices n'y voient aucune adjacence ; les positions
    /// soudées la retrouvent. *Sans cette garde, le banc aurait conclu « pas de connectivité » sur
    /// un maillage parfaitement connexe.*
    #[test]
    fn la_soudure_retrouve_l_adjacence_qu_une_couture_avait_coupee() {
        let positions = [
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 0.0, 0.0], // doublon de 0
            [1.0, 1.0, 0.0], // doublon de 2
            [0.0, 1.0, 0.0],
        ];
        let indices = [0, 1, 2, 3, 4, 5];

        let brut = topologie(&indices, &|i| i);
        assert_eq!(brut.manifold, 0, "les indices seuls ne voient aucune adjacence");
        assert_eq!(brut.bord, 6, "six bords, soit deux triangles isolés");

        let table = souder(&positions);
        let soude = topologie(&indices, &|i| table[i as usize]);
        assert_eq!(soude.manifold, 1, "la soudure retrouve la diagonale partagée");
        assert_eq!(soude.bord, 4, "et le carré retrouve ses quatre bords");
    }

    /// `-0.0` et `+0.0` sont le même point : la soudure ne doit pas les séparer.
    #[test]
    fn les_deux_zeros_sont_le_meme_point() {
        let table = souder(&[[0.0, 0.0, 0.0], [-0.0, 0.0, -0.0]]);
        assert_eq!(table[0], table[1]);
    }

    /// L'aire d'un triangle rectangle de côtés 3 et 4 vaut 6 — vérité de collège.
    #[test]
    fn l_aire_est_confrontee_a_une_verite_analytique() {
        let a = aire([0.0, 0.0, 0.0], [3.0, 0.0, 0.0], [0.0, 4.0, 0.0]);
        assert!((a - 6.0).abs() < 1e-9, "aire = {a}, attendue 6");
    }

    /// Un triangle quatre fois plus grand que la cible demande UN niveau de subdivision : il donne
    /// alors 4 micro-triangles d'aire exactement égale à la cible.
    #[test]
    fn le_niveau_de_subdivision_suit_la_formule_du_corpus() {
        assert_eq!(niveau_subdivision(1.0, 1.0), 0, "déjà à la cible");
        assert_eq!(niveau_subdivision(0.5, 1.0), 0, "plus petit que la cible");
        assert_eq!(niveau_subdivision(4.0, 1.0), 1, "4× la cible → 4 micro-triangles");
        assert_eq!(niveau_subdivision(16.0, 1.0), 2, "16× la cible → 16 micro-triangles");
    }

    /// ⭐ LE PLANCHER, énoncé comme test : quelle que soit la grossièreté visée, un micro-maillage
    /// porte au moins autant d'échantillons que le maillage a de triangles. **C'est ce qui fait
    /// perdre la voie barycentrique dès que la lumière est plus grossière que la géométrie**, et
    /// c'est une propriété de l'arithmétique, pas du modèle mesuré.
    #[test]
    fn le_micro_maillage_ne_descend_jamais_sous_le_nombre_de_triangles() {
        let aires = [1e-6, 1e-4, 1e-2, 1.0];
        for cible in [1.0, 10.0, 1e6] {
            let total: u64 = aires
                .iter()
                .map(|a| 4u64.saturating_pow(niveau_subdivision(*a, cible)))
                .sum();
            assert!(
                total >= aires.len() as u64,
                "cible {cible} : {total} échantillons pour {} triangles",
                aires.len()
            );
        }
    }
}

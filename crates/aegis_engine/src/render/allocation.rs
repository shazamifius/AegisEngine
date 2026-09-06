//! **L'ALLOCATION — le chantier 0.2, et c'est le problème dur de l'étage 0.**
//!
//! > *« Le problème dur est l'ALLOCATION — combien de mémoire donner à quelle surface, et quand.
//! > C'est ce à quoi OSC-GI consacre son papier. »* — `02-THESE.md` § Le prix
//!
//! Le chantier 0.1 a donné l'adresse ; il supposait la subdivision **uniforme**. C'est intenable :
//! le banc `topologie` mesure des aires de triangles qui varient de **20 610 ×** sur un `.glb`
//! Blender ordinaire, et le banc `lire_surface` chiffre le prix de l'uniformité — **56 Mo** pour
//! une seule table à $k = 6$.
//!
//! *Une densité uniforme est absurde des deux côtés à la fois : famélique sur une petite pièce vue
//! de près, ruineuse sur un grand mur vu de loin.*
//!
//! ## ⭐ La règle, et elle ne vient pas de nous
//!
//! Trois sources indépendantes convergent sur **le même critère**, et aucune ne visait notre
//! problème :
//!
//! | Qui | Ce qu'ils imposent |
//! |---|---|
//! | **FastAtlas** (CGF 2025) | *« notre paramétrisation garantit un ratio texel-à-pixel CONSTANT »*, et un facteur d'échelle **global** cherché pour que tout tienne dans l'atlas |
//! | **Split Radiance Cascades** (2026) | les sondes sont semées en parcourant les **pixels** ; leur nombre est *« quasi constant grâce aux LOD »* |
//! | **OSC-GI** (HPG 2024) | un *mip bias* règle la taille des entrées — *« notre objectif est la qualité visuelle optimale à résolution d'espace texture MINIMALE »* |
//!
//! > ### 🔺 La règle retenue : un micro-sommet par pixel d'écran, et un seul nombre pour tout régler.
//!
//! ## La dérivation — et le biais n'est pas réglé, il est CALCULÉ
//!
//! Un triangle qui couvre $A$ pixels à l'écran veut $A$ micro-sommets. Un triangle subdivisé $k$
//! fois en porte $(2^k+1)(2^k+2)/2 \approx 4^k/2$. D'où la subdivision idéale :
//!
//! $$k_{\text{ideal}}(T) = \left\lceil \tfrac{1}{2}\log_2\!\big(2\,A(T)\big) \right\rceil$$
//!
//! Mais la somme de ces $k_{\text{ideal}}$ ne tient pas forcément dans le budget. On applique donc
//! un **biais global** $b$, entier, identique pour tous les triangles :
//!
//! $$k(T) = \mathrm{clamp}\big(k_{\text{ideal}}(T) + b,\; k_{\min},\; k_{\max}\big)$$
//!
//! ⭐⭐ **Et $b$ ne se règle pas : il se dérive.** Comme le coût d'un triangle est en $4^k$, baisser
//! $b$ d'une unité divise l'empreinte par ~4. Le bon $b$ est donc le plus grand qui tienne dans le
//! budget, et on le trouve en l'essayant par valeurs décroissantes — au plus une poignée d'essais,
//! sur un calcul qui est une simple somme.
//!
//! *C'est ce que le corpus appelle une constante qui **disparaît** au lieu de rétrécir : il n'y a
//! aucun « nombre de texels par objet » à justifier, aucune heuristique de remplissage. Il y a un
//! budget d'octets, et une densité qui s'y plie.*
//!
//! ## ⚠⚠ CE QUE CE FICHIER NE RÉSOUT PAS — et c'est nommé avant d'être mesuré
//!
//! **Deux triangles voisins peuvent recevoir des subdivisions différentes.** Leur arête commune
//! porte alors des micro-sommets à deux densités qui ne coïncident pas — *donc une couture, très
//! exactement ce que l'adresse barycentrique était censée supprimer.*
//!
//! C'est un problème **connu et résolu ailleurs** : les micro-maillages de NVIDIA imposent
//! l'étanchéité en alignant le niveau d'une arête sur le **minimum** des deux triangles qui la
//! partagent, et Nanite verrouille les bords de ses clusters. **Rien de tout cela n'est fait ici.**
//!
//! ### ✅ MESURÉ, puis FERMÉ — 6 septembre 2026
//!
//! La couture a d'abord été **mesurée** : 2,3× à 5,4× le bruit de fond des arêtes dures. Puis
//! [`raccorder`] l'a fermée, par la règle des micro-maillages — *le niveau d'une arête est le
//! **minimum** des deux triangles qui la partagent, et le plus fin décime son bord pour
//! correspondre au plus grossier.*

use crate::render::surface::micro_sommets;

/// La subdivision plancher : trois coins, aucun micro-sommet intérieur.
///
/// *Un triangle ne peut pas porter moins que ses propres sommets — c'est le plancher structurel que
/// le journal `0.a` a nommé, et il est ici une constante de type, pas un choix.*
pub const K_MIN: u32 = 0;

/// La subdivision plafond.
///
/// ⚠ Ce n'est pas une limite de qualité, c'est une limite d'ARITHMÉTIQUE : à $k = 10$ un seul
/// triangle porterait 527 000 micro-sommets, et le rang d'un micro-sommet approche les bornes où la
/// racine carrée en simple précision de `depuis_rang` cesse d'être fiable. *Le test de bijection ne
/// couvre que jusqu'à `k = 6` ; au-delà, rien n'est vérifié, et un plafond honnête vaut mieux qu'une
/// confiance non mesurée.*
pub const K_MAX: u32 = 8;

/// Le plan d'allocation : pour chaque triangle, sa subdivision et l'adresse où sa plage commence.
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    /// Par triangle : `(base, cote)` où `cote` $= 2^{k}$.
    ///
    /// *Deux `u32` par triangle, et c'est le prix de la non-uniformité — 8 octets qui n'existaient
    /// pas quand $k$ était le même pour tous. Sur une scène de 3 274 triangles : 26 Ko.*
    pub par_triangle: Vec<(u32, u32)>,
    /// Par triangle et par arête, la subdivision EFFECTIVE de son bord, en segments.
    ///
    /// Vaut la subdivision du triangle tant que [`raccorder`] n'a pas tourné ; ensuite, le
    /// **minimum** des deux triangles qui partagent l'arête. L'ordre des arêtes est celui des coins :
    /// `[ (v0,v1), (v1,v2), (v2,v0) ]`.
    pub aretes: Vec<[u32; 3]>,
    /// Le total des micro-sommets, tous triangles confondus.
    pub entrees: u32,
    /// Le biais appliqué à la subdivision idéale pour tenir le budget.
    pub biais: i32,
    /// Le nombre de triangles dont la subdivision a été écrêtée par [`K_MAX`].
    ///
    /// *Un compteur, pas une alerte : il dit si le plafond MORD, ce qu'aucune relecture ne dirait.*
    pub ecretes: u32,
}

impl Plan {
    pub fn octets(&self) -> u64 {
        self.entrees as u64 * crate::render::surface::OCTETS_PAR_ENTREE
    }

    /// La subdivision d'un triangle, en nombre de segments par arête.
    pub fn cote(&self, triangle: u32) -> u32 {
        self.par_triangle[triangle as usize].1
    }
}

/// La subdivision idéale pour un triangle qui couvre `aire_ecran` pixels.
///
/// *Un micro-sommet par pixel : $4^k/2 \ge A \Rightarrow k \ge \frac{1}{2}\log_2(2A)$.*
///
/// ⚠ Un triangle dos à la caméra, hors champ ou dégénéré rend une aire nulle ou négative — il reçoit
/// alors [`K_MIN`]. *Il garde ses trois coins : la mémoire de surface est attachée à la géométrie,
/// pas au regard, et une surface qu'on ne voit pas peut redevenir visible à l'image suivante.*
pub fn k_ideal(aire_ecran: f32) -> u32 {
    // ⚠ `is_nan()` explicite plutôt qu'un `!(x > 0.5)` : un NaN doit tomber au plancher, et une
    // comparaison niée sur un flottant cache ce cas au lieu de le traiter.
    if aire_ecran.is_nan() || aire_ecran <= 0.5 {
        return K_MIN;
    }
    let k = 0.5 * (2.0 * aire_ecran).log2();
    (k.ceil().max(0.0) as u32).min(K_MAX)
}

/// Construit le plan d'allocation d'une scène pour un budget d'octets donné.
///
/// `aires` porte, pour chaque triangle, sa surface projetée à l'écran en pixels.
///
/// ## ⭐ Comment le biais est trouvé
///
/// On part du biais nul — la densité idéale — et on descend tant que l'empreinte dépasse le budget.
/// Chaque pas divise l'empreinte par ~4, donc la boucle est courte par construction : de la densité
/// idéale au plancher, il y a au plus [`K_MAX`] pas.
///
/// **Le biais ne monte jamais au-dessus de zéro.** Un budget généreux ne doit pas produire une
/// densité plus fine que le pixel : ce serait de l'excédent au sens strict — de la mémoire dépensée
/// pour une information que l'écran ne peut pas montrer.
pub fn planifier(aires: &[f32], budget_octets: u64) -> Plan {
    let ideaux: Vec<u32> = aires.iter().map(|a| k_ideal(*a)).collect();

    let cout = |biais: i32| -> u64 {
        ideaux
            .iter()
            .map(|k| {
                let ajuste = (*k as i32 + biais).clamp(K_MIN as i32, K_MAX as i32) as u32;
                micro_sommets(ajuste) as u64
            })
            .sum::<u64>()
            * crate::render::surface::OCTETS_PAR_ENTREE
    };

    let mut biais = 0i32;
    while biais > -(K_MAX as i32) && cout(biais) > budget_octets {
        biais -= 1;
    }

    let mut par_triangle = Vec::with_capacity(ideaux.len());
    let mut base = 0u32;
    let mut ecretes = 0u32;
    for k in &ideaux {
        let brut = *k as i32 + biais;
        let ajuste = brut.clamp(K_MIN as i32, K_MAX as i32) as u32;
        if brut > K_MAX as i32 {
            ecretes += 1;
        }
        par_triangle.push((base, 1u32 << ajuste));
        base += micro_sommets(ajuste);
    }

    // Sans raccord, le bord d'un triangle a sa propre subdivision — d'où la couture.
    let aretes = par_triangle.iter().map(|(_, c)| [*c, *c, *c]).collect();
    Plan { par_triangle, aretes, entrees: base, biais, ecretes }
}

/// ⭐⭐⭐ **LE RACCORD DES ARÊTES — ce qui rend l'étage 0 étanche.**
///
/// Sans lui, deux triangles voisins de subdivisions différentes interpolent le long de leur arête
/// commune entre des micro-sommets qui **ne coïncident pas**. La discontinuité est mathématiquement
/// certaine, et elle a été mesurée à **2,3–5,4×** le bruit de fond avant d'être fermée.
///
/// ## La règle, et elle est celle des micro-maillages de NVIDIA
///
/// > **Le niveau d'une arête est le MINIMUM des deux triangles qui la partagent.**
///
/// Le triangle le plus fin **décime** son bord : les micro-sommets qui ne tombent pas sur le pas
/// grossier cessent de porter leur propre valeur et sont interpolés linéairement entre leurs deux
/// voisins alignés. *Les deux côtés font alors la même interpolation linéaire entre les mêmes
/// points — l'égalité n'est pas approchée, elle est exacte.*
///
/// ⭐ **Et rien n'est perdu à l'intérieur du triangle** : seul son BORD est décimé. Un grand
/// triangle finement subdivisé garde toute sa densité intérieure ; il ne cède que sur la ligne où
/// il doit s'accorder avec son voisin.
///
/// ## ⚠ Les arêtes se trouvent par SOUDURE des positions, jamais par les indices
///
/// Le banc `topologie` mesure **73,5 % de sommets dupliqués** par l'exportateur Blender : une
/// recherche par indices ne verrait que **37 %** de l'adjacence réelle, raccorderait un tiers des
/// arêtes et laisserait les autres coutures en place — *en donnant l'impression d'avoir traité le
/// sujet.*
///
/// ⚠ La soudure est **exacte, bit à bit**. Aucune tolérance à régler : deux sommets sont le même
/// point ou ils ne le sont pas. *Une tolérance ferait fusionner deux surfaces distinctes qui se
/// touchent, et inventerait une adjacence — le banc `topologie` met en garde contre exactement ça.*
pub fn raccorder(plan: &mut Plan, positions: &[[f32; 3]], indices: &[u32]) {
    use std::collections::HashMap;

    let cle = |p: &[f32; 3]| (p[0].to_bits(), p[1].to_bits(), p[2].to_bits());
    let mut soude: HashMap<(u32, u32, u32), u32> = HashMap::new();
    let mut canonique = vec![0u32; positions.len()];
    for (i, p) in positions.iter().enumerate() {
        let n = soude.len() as u32;
        canonique[i] = *soude.entry(cle(p)).or_insert(n);
    }

    // Le niveau minimal rencontré sur chaque arête soudée.
    let mut minimum: HashMap<(u32, u32), u32> = HashMap::new();
    for (t, tri) in indices.chunks_exact(3).enumerate() {
        let cote = plan.par_triangle[t].1;
        for c in 0..3usize {
            let a = canonique[tri[c] as usize];
            let b = canonique[tri[(c + 1) % 3] as usize];
            let e = minimum.entry((a.min(b), a.max(b))).or_insert(cote);
            *e = (*e).min(cote);
        }
    }

    for (t, tri) in indices.chunks_exact(3).enumerate() {
        for c in 0..3usize {
            let a = canonique[tri[c] as usize];
            let b = canonique[tri[(c + 1) % 3] as usize];
            plan.aretes[t][c] = minimum[&(a.min(b), a.max(b))];
        }
    }
}

/// Encode le plan pour le GPU : deux `u32` par triangle.
///
/// Le second mot porte la subdivision du triangle dans ses 16 bits bas, et les **exposants** des
/// trois arêtes sur 4 bits chacun dans les bits hauts.
///
/// *Quatre bits suffisent puisque [`K_MAX`] vaut 8 — et les empaqueter ici plutôt que d'ajouter un
/// troisième mot garde le plan à 8 octets par triangle, ce qui était son coût avant le raccord.*
pub fn encoder_pour_gpu(plan: &Plan) -> Vec<u32> {
    plan.par_triangle
        .iter()
        .zip(plan.aretes.iter())
        .flat_map(|((base, cote), aretes)| {
            let k = |c: u32| c.trailing_zeros();
            [
                *base,
                (*cote & 0xffff) | (k(aretes[0]) << 16) | (k(aretes[1]) << 20) | (k(aretes[2]) << 24),
            ]
        })
        .collect()
}

/// Les aires projetées à l'écran de chaque triangle, en pixels.
///
/// ⚠ **L'aire est prise en valeur absolue.** Un triangle vu de dos a une aire signée négative ; il
/// n'en couvre pas moins de pixels, et le culler n'est pas le rôle de ce calcul. *Confondre
/// « orienté vers l'arrière » et « invisible » ferait disparaître la mémoire de surface d'un objet
/// qu'on regarde par l'intérieur.*
///
/// Un sommet derrière le plan de la caméra rend l'aire **nulle** plutôt qu'une valeur absurde : la
/// division par $w \le 0$ retourne la projection. *Le triangle reçoit alors [`K_MIN`], et il sera
/// re-planifié quand il reviendra devant.*
pub fn aires_ecran(
    positions: &[[f32; 3]],
    indices: &[u32],
    view_proj: &[f32; 16],
    largeur: f32,
    hauteur: f32,
) -> Vec<f32> {
    let projeter = |p: &[f32; 3]| -> Option<(f32, f32)> {
        let m = view_proj;
        // Colonnes majeures, comme `mat4x4<f32>` en WGSL.
        let x = m[0] * p[0] + m[4] * p[1] + m[8] * p[2] + m[12];
        let y = m[1] * p[0] + m[5] * p[1] + m[9] * p[2] + m[13];
        let w = m[3] * p[0] + m[7] * p[1] + m[11] * p[2] + m[15];
        if w <= 1e-6 {
            return None;
        }
        Some((x / w * 0.5 * largeur, y / w * 0.5 * hauteur))
    };

    indices
        .chunks_exact(3)
        .map(|t| {
            let (Some(a), Some(b), Some(c)) = (
                projeter(&positions[t[0] as usize]),
                projeter(&positions[t[1] as usize]),
                projeter(&positions[t[2] as usize]),
            ) else {
                return 0.0;
            };
            0.5 * ((b.0 - a.0) * (c.1 - a.1) - (c.0 - a.0) * (b.1 - a.1)).abs()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_subdivision_ideale_suit_l_aire_a_l_ecran() {
        // 4^k/2 micro-sommets pour A pixels : k = 1 couvre 2 px, k = 2 en couvre 8, k = 3 en couvre 32.
        assert_eq!(k_ideal(0.0), K_MIN, "un triangle invisible garde ses trois coins");
        assert_eq!(k_ideal(2.0), 1);
        assert_eq!(k_ideal(8.0), 2);
        assert_eq!(k_ideal(32.0), 3);
        assert_eq!(k_ideal(1e9), K_MAX, "le plafond tient face à une aire absurde");
    }

    /// ⭐ **La garde qui porte le critère d'invalidation de la thèse.**
    ///
    /// `02-THESE.md` écrit noir sur blanc : *« ce qui invaliderait : une empreinte NON BORNÉE »*.
    /// Ce test est cette phrase, rendue exécutable — sur des aires qui vont de la poussière au mur
    /// entier, l'empreinte doit tenir dans le budget qu'on lui donne, quel qu'il soit.
    #[test]
    fn l_empreinte_tient_toujours_dans_le_budget() {
        let aires: Vec<f32> = (0..5000)
            .map(|i| (i as f32 * 0.37).sin().abs() * 10_000.0 + 0.01)
            .collect();
        for budget in [4_096u64, 65_536, 1_000_000, 16_000_000, 512_000_000] {
            let plan = planifier(&aires, budget);
            assert!(
                plan.octets() <= budget || plan.biais == -(K_MAX as i32),
                "budget {budget} dépassé : {} octets, biais {}",
                plan.octets(),
                plan.biais
            );
        }
    }

    /// Le plancher est structurel : même avec un budget d'un octet, chaque triangle garde ses coins.
    ///
    /// *C'est le « plancher du micro-maillage » que le journal `0.a` a nommé comme un résultat
    /// négatif. Il est ici une propriété vérifiée, pas une surprise à venir.*
    #[test]
    fn un_budget_insuffisant_rend_le_plancher_et_ne_ment_pas() {
        let aires = vec![100_000.0f32; 1000];
        let plan = planifier(&aires, 1);
        assert_eq!(plan.biais, -(K_MAX as i32));
        assert!(plan.octets() > 1, "le plancher dépasse forcément un budget d'un octet");
        assert!(
            plan.par_triangle.iter().all(|(_, cote)| *cote == 1),
            "chaque triangle doit être retombé au plancher"
        );
    }

    /// ⭐ Les bases doivent se suivre sans trou ni recouvrement — sinon deux triangles s'écrasent.
    #[test]
    fn les_plages_se_suivent_sans_trou_ni_recouvrement() {
        let aires: Vec<f32> = (0..300).map(|i| (i as f32).powf(1.7) + 0.5).collect();
        let plan = planifier(&aires, 8_000_000);
        let mut attendu = 0u32;
        for (i, (base, cote)) in plan.par_triangle.iter().enumerate() {
            assert_eq!(*base, attendu, "trou ou recouvrement au triangle {i}");
            attendu += micro_sommets(cote.trailing_zeros());
        }
        assert_eq!(plan.entrees, attendu);
    }

    /// Un budget plus grand ne doit jamais rendre une empreinte plus petite.
    ///
    /// *Une allocation qui n'est pas monotone dans son budget est une allocation qui oscille — et
    /// l'hystérésis que l'adaptativité exige n'aurait alors rien de stable à quoi s'accrocher.*
    #[test]
    fn plus_de_budget_ne_donne_jamais_moins_de_densite() {
        let aires: Vec<f32> = (0..800).map(|i| ((i * 7) % 500) as f32 + 1.0).collect();
        let mut precedent = 0u64;
        for budget in [10_000u64, 100_000, 1_000_000, 10_000_000, 100_000_000] {
            let plan = planifier(&aires, budget);
            assert!(
                plan.octets() >= precedent,
                "budget {budget} rend {} octets, moins que le budget précédent ({precedent})",
                plan.octets()
            );
            precedent = plan.octets();
        }
    }

    /// ⚠ La densité ne doit JAMAIS dépasser le pixel, même avec un budget infini.
    ///
    /// *Sinon c'est de l'excédent au sens strict de la règle du projet : de la mémoire dépensée pour
    /// une information que l'écran ne peut pas montrer.*
    #[test]
    fn un_budget_genereux_ne_produit_aucun_excedent() {
        let aires = vec![64.0f32; 100];
        let plan = planifier(&aires, u64::MAX / 2);
        assert_eq!(plan.biais, 0, "le biais ne doit jamais monter au-dessus de zéro");
        let attendu = 1u32 << k_ideal(64.0);
        assert!(plan.par_triangle.iter().all(|(_, cote)| *cote == attendu));
    }

    /// ⭐⭐ **La garde du raccord : deux triangles voisins doivent voir le MÊME niveau d'arête.**
    ///
    /// C'est la propriété dont dépend l'étanchéité de l'étage 0 : si les deux côtés d'une arête
    /// n'ont pas le même niveau effectif, ils interpolent entre des points différents, et la couture
    /// revient. *Le banc `lire_surface` mesure l'effet ; ce test garde la CAUSE, et il tourne sans
    /// GPU.*
    #[test]
    fn les_deux_cotes_d_une_arete_voient_le_meme_niveau() {
        // Deux triangles qui partagent l'arête (1,2), avec des aires très différentes — donc des
        // subdivisions très différentes avant raccord.
        let positions = [
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [1.0, 1.0, 0.0],
        ];
        let indices = [0, 1, 2, 1, 3, 2];
        let mut plan = planifier(&[10_000.0, 4.0], u64::MAX / 2);
        assert_ne!(
            plan.par_triangle[0].1, plan.par_triangle[1].1,
            "le cas de test doit produire deux subdivisions différentes, sinon il ne teste rien"
        );
        raccorder(&mut plan, &positions, &indices);

        // L'arête partagée : (1,2) est l'arête 1 du premier triangle et l'arête 2 du second.
        assert_eq!(
            plan.aretes[0][1], plan.aretes[1][2],
            "les deux côtés de l'arête partagée doivent porter le même niveau"
        );
        assert_eq!(
            plan.aretes[0][1],
            plan.par_triangle[0].1.min(plan.par_triangle[1].1),
            "le niveau d'une arête doit être le MINIMUM des deux triangles"
        );
        // ⚠ Et l'intérieur ne doit PAS avoir été décimé : seul le bord cède.
        assert_eq!(plan.aretes[0][0], plan.par_triangle[0].1, "une arête de BORD garde son niveau");
    }

    /// ⚠ Le raccord doit tenir sur un maillage dont l'exportateur a DUPLIQUÉ les sommets.
    ///
    /// *Le banc `topologie` mesure 73,5 % de duplication : un raccord qui cherche l'adjacence par
    /// les indices ne verrait que 37 % des arêtes, en raccorderait un tiers, et laisserait les
    /// autres coutures en place — **avec l'air d'avoir traité le sujet**.*
    #[test]
    fn le_raccord_survit_aux_sommets_dupliques_par_l_exportateur() {
        // Les mêmes deux triangles, mais chacun avec ses propres sommets — aucun indice partagé.
        let positions = [
            [0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0],
            [1.0, 0.0, 0.0], [1.0, 1.0, 0.0], [0.0, 1.0, 0.0],
        ];
        let indices = [0, 1, 2, 3, 4, 5];
        let mut plan = planifier(&[10_000.0, 4.0], u64::MAX / 2);
        raccorder(&mut plan, &positions, &indices);
        assert_eq!(
            plan.aretes[0][1], plan.aretes[1][2],
            "l'adjacence doit être retrouvée par la POSITION, pas par les indices"
        );
    }

    /// L'empaquetage GPU doit se relire exactement — sinon le shader lit une adresse absurde.
    #[test]
    fn l_encodage_gpu_se_relit_sans_perte() {
        let mut plan = planifier(&[10_000.0, 4.0, 250.0], u64::MAX / 2);
        raccorder(
            &mut plan,
            &[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [1.0, 1.0, 0.0], [2.0, 0.0, 0.0]],
            &[0, 1, 2, 1, 3, 2, 1, 4, 3],
        );
        let mots = encoder_pour_gpu(&plan);
        for (t, ((base, cote), aretes)) in plan.par_triangle.iter().zip(&plan.aretes).enumerate() {
            assert_eq!(mots[t * 2], *base);
            let mot = mots[t * 2 + 1];
            assert_eq!(mot & 0xffff, *cote, "la subdivision doit vivre dans les 16 bits bas");
            for (e, niveau) in aretes.iter().enumerate() {
                let k = (mot >> (16 + 4 * e)) & 0xf;
                assert_eq!(1u32 << k, *niveau, "l\'arête {e} du triangle {t} se relit mal");
            }
        }
    }

    /// Une aire nulle ou un sommet derrière la caméra ne doit pas produire de valeur absurde.
    #[test]
    fn un_triangle_derriere_la_camera_ne_produit_aucune_aire() {
        // Une projection identité : w vaut alors la composante homogène, soit 1 pour tout point.
        let identite = [
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0f32,
        ];
        let positions = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        let aires = aires_ecran(&positions, &[0, 1, 2], &identite, 100.0, 100.0);
        assert!(aires[0] > 0.0, "un triangle devant la caméra doit couvrir une aire");

        // La même chose, mais avec un w nul : la projection doit refuser plutôt que diviser.
        let degeneree = [
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0f32,
        ];
        let aires = aires_ecran(&positions, &[0, 1, 2], &degeneree, 100.0, 100.0);
        assert_eq!(aires[0], 0.0, "un w nul doit rendre une aire nulle, pas un infini");
    }

    /// ⭐ L'aire est prise en valeur absolue : un triangle vu de dos couvre autant de pixels.
    #[test]
    fn un_triangle_vu_de_dos_couvre_la_meme_aire() {
        let identite = [
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0f32,
        ];
        let positions = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        let direct = aires_ecran(&positions, &[0, 1, 2], &identite, 100.0, 100.0)[0];
        let inverse = aires_ecran(&positions, &[0, 2, 1], &identite, 100.0, 100.0)[0];
        assert_eq!(direct, inverse);
    }
}

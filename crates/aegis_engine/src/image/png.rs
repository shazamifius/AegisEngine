//! # Écrire un PNG, en Rust, sans rien demander à personne
//!
//! ## ⚠⚠ Ce que ce fichier remplace, et pourquoi c'était grave
//!
//! Jusqu'au 30 août 2026, capturer l'écran d'Aegis **écrivait un script Python sur le disque et
//! lançait `python3` dessus** pour encoder le PNG. Dans le binaire livré, à l'exécution.
//!
//! Trois fautes en une :
//!
//! 1. **La règle la plus ferme du projet est « QUE du Rust, aucun autre langage ».** Elle a été
//!    posée après un `pip install` — et elle interdit explicitement l'excuse « ce n'est qu'un
//!    outil, ce n'est pas embarqué ». Ici ce n'était même pas un outil de build : c'était du code
//!    exécuté chez le joueur.
//! 2. **La capture échouait chez quiconque n'a pas `python3`** — la majorité des machines
//!    Windows, par exemple.
//! 3. **Et cet échec était AVALÉ** : le code ne journalisait que le succès. Pas de branche `else`,
//!    pas d'avertissement. *Un échec avalé est le meilleur endroit où un mécanisme meurt sans
//!    témoin*, et celui-ci serait mort chez quelqu'un d'autre, en silence.
//!
//! ## Ce que ça écrit
//!
//! Un PNG couleur 8 bits par canal, avec un vrai flux `deflate` : LZ77 (fenêtre de 32 Ko, table de
//! hachage sur trois octets) et l'arbre de Huffman **fixe** de la spécification. L'arbre fixe est
//! un choix, pas un raccourci : construire un arbre dynamique gagnerait quelques pourcents sur une
//! image d'aplats, au prix d'un code trois fois plus long et d'une classe entière de fautes
//! silencieuses. *L'élégance, ici, est de s'arrêter au point où le gain cesse de payer sa
//! complexité.*
//!
//! ## Comment on sait que c'est juste
//!
//! Un encodeur qui produit un fichier « qui a l'air d'un PNG » ne prouve rien — c'est exactement
//! la famille de sondes qui mentent (une taille, une présence, un format ne disent jamais le
//! contenu). Ce module porte donc **son propre décodeur**, réservé aux tests, et les deux se
//! contrôlent l'un l'autre : ce qui est écrit doit se relire **octet pour octet**. Un aller simple
//! ne prouverait rien ; l'aller-retour, si.

/// Le tableau du CRC-32 de la spécification PNG, calculé une fois à la demande.
fn table_crc() -> [u32; 256] {
    let mut table = [0u32; 256];
    let mut n = 0;
    while n < 256 {
        let mut c = n as u32;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
            k += 1;
        }
        table[n] = c;
        n += 1;
    }
    table
}

fn crc32(donnees: &[u8]) -> u32 {
    let table = table_crc();
    let mut c = 0xFFFF_FFFFu32;
    for &octet in donnees {
        c = table[((c ^ u32::from(octet)) & 0xFF) as usize] ^ (c >> 8);
    }
    c ^ 0xFFFF_FFFF
}

/// La somme de contrôle du flux zlib — elle porte sur les données AVANT compression.
fn adler32(donnees: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for &octet in donnees {
        a = (a + u32::from(octet)) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

/// Écrit des bits dans l'ordre qu'attend `deflate` : du bit de poids faible vers le fort.
///
/// ⚠ **Les codes de Huffman, eux, s'écrivent dans l'autre sens** (du bit de poids fort au faible).
/// Mélanger les deux est LE piège de ce format : le flux reste plausible, sa taille est correcte,
/// et il ne se décompresse pas. Les deux méthodes ci-dessous existent pour rendre l'erreur
/// impossible à commettre par distraction.
struct Bits {
    octets: Vec<u8>,
    courant: u32,
    combien: u32,
}

impl Bits {
    fn nouveau() -> Self {
        Self { octets: Vec::new(), courant: 0, combien: 0 }
    }

    /// Pour tout ce qui n'est pas un code de Huffman : longueurs, distances, en-têtes de bloc.
    fn ecrire(&mut self, valeur: u32, largeur: u32) {
        self.courant |= valeur << self.combien;
        self.combien += largeur;
        while self.combien >= 8 {
            self.octets.push((self.courant & 0xFF) as u8);
            self.courant >>= 8;
            self.combien -= 8;
        }
    }

    /// Pour un code de Huffman, dont les bits sont donnés du plus significatif au moins.
    fn ecrire_huffman(&mut self, code: u32, largeur: u32) {
        for i in (0..largeur).rev() {
            self.ecrire((code >> i) & 1, 1);
        }
    }

    fn terminer(mut self) -> Vec<u8> {
        if self.combien > 0 {
            self.octets.push((self.courant & 0xFF) as u8);
        }
        self.octets
    }
}

/// Le code de Huffman fixe d'un littéral ou d'une longueur, tel que la spécification le définit.
fn code_fixe(symbole: u16) -> (u32, u32) {
    match symbole {
        0..=143 => (0x30 + u32::from(symbole), 8),
        144..=255 => (0x190 + u32::from(symbole) - 144, 9),
        256..=279 => (u32::from(symbole) - 256, 7),
        _ => (0xC0 + u32::from(symbole) - 280, 8),
    }
}

/// Les 29 classes de longueur : symbole, longueur minimale, nombre de bits supplémentaires.
const LONGUEURS: [(u16, u16, u32); 29] = [
    (257, 3, 0), (258, 4, 0), (259, 5, 0), (260, 6, 0), (261, 7, 0), (262, 8, 0),
    (263, 9, 0), (264, 10, 0), (265, 11, 1), (266, 13, 1), (267, 15, 1), (268, 17, 1),
    (269, 19, 2), (270, 23, 2), (271, 27, 2), (272, 31, 2), (273, 35, 3), (274, 43, 3),
    (275, 51, 3), (276, 59, 3), (277, 67, 4), (278, 83, 4), (279, 99, 4), (280, 115, 4),
    (281, 131, 5), (282, 163, 5), (283, 195, 5), (284, 227, 5), (285, 258, 0),
];

/// Les 30 classes de distance : symbole, distance minimale, nombre de bits supplémentaires.
const DISTANCES: [(u16, u32, u32); 30] = [
    (0, 1, 0), (1, 2, 0), (2, 3, 0), (3, 4, 0), (4, 5, 1), (5, 7, 1), (6, 9, 2), (7, 13, 2),
    (8, 17, 3), (9, 25, 3), (10, 33, 4), (11, 49, 4), (12, 65, 5), (13, 97, 5),
    (14, 129, 6), (15, 193, 6), (16, 257, 7), (17, 385, 7), (18, 513, 8), (19, 769, 8),
    (20, 1025, 9), (21, 1537, 9), (22, 2049, 10), (23, 3073, 10), (24, 4097, 11),
    (25, 6145, 11), (26, 8193, 12), (27, 12289, 12), (28, 16385, 13), (29, 24577, 13),
];

const FENETRE: usize = 32768;
const LONGUEUR_MAX: usize = 258;

/// Compresse en `deflate`, arbre de Huffman fixe, un seul bloc final.
fn deflate(donnees: &[u8]) -> Vec<u8> {
    let mut bits = Bits::nouveau();
    // En-tête de bloc : dernier bloc (1), méthode « Huffman fixe » (01).
    bits.ecrire(1, 1);
    bits.ecrire(1, 2);

    // La table de hachage garde, pour chaque triplet d'octets, les positions où on l'a déjà vu.
    // Une chaîne bornée suffit : chercher plus loin coûte du temps pour un gain qui s'effondre.
    let mut vues: std::collections::HashMap<[u8; 3], Vec<usize>> = std::collections::HashMap::new();
    let mut i = 0usize;

    while i < donnees.len() {
        let mut meilleure = (0usize, 0usize); // (longueur, distance)

        if i + 3 <= donnees.len() {
            let clef = [donnees[i], donnees[i + 1], donnees[i + 2]];
            if let Some(positions) = vues.get(&clef) {
                // Du plus récent au plus ancien : une distance courte coûte moins de bits.
                for &p in positions.iter().rev().take(48) {
                    let distance = i - p;
                    if distance > FENETRE {
                        break;
                    }
                    let mut longueur = 0usize;
                    while longueur < LONGUEUR_MAX
                        && i + longueur < donnees.len()
                        && donnees[p + longueur] == donnees[i + longueur]
                    {
                        longueur += 1;
                    }
                    if longueur > meilleure.0 {
                        meilleure = (longueur, distance);
                        if longueur == LONGUEUR_MAX {
                            break;
                        }
                    }
                }
            }
        }

        if meilleure.0 >= 3 {
            let (longueur, distance) = meilleure;
            let (sym_l, base_l, extra_l) = *LONGUEURS
                .iter()
                .rev()
                .find(|(_, base, _)| usize::from(*base) <= longueur)
                .expect("toute longueur de 3 a 258 tombe dans une classe");
            let (code, largeur) = code_fixe(sym_l);
            bits.ecrire_huffman(code, largeur);
            if extra_l > 0 {
                bits.ecrire((longueur - usize::from(base_l)) as u32, extra_l);
            }

            let (sym_d, base_d, extra_d) = *DISTANCES
                .iter()
                .rev()
                .find(|(_, base, _)| *base as usize <= distance)
                .expect("toute distance de 1 a 32768 tombe dans une classe");
            // ⚠ Les distances ont leur PROPRE arbre fixe : cinq bits bruts, pas de code de
            // Huffman. Leur appliquer l'arbre des littéraux produit un flux illisible.
            bits.ecrire_huffman(u32::from(sym_d), 5);
            if extra_d > 0 {
                bits.ecrire(distance as u32 - base_d, extra_d);
            }

            // Toutes les positions couvertes doivent entrer dans la table, sinon les
            // correspondances suivantes deviennent aveugles à ce qui vient d'être écrit.
            for j in i..(i + longueur).min(donnees.len().saturating_sub(2)) {
                vues.entry([donnees[j], donnees[j + 1], donnees[j + 2]]).or_default().push(j);
            }
            i += longueur;
        } else {
            let (code, largeur) = code_fixe(u16::from(donnees[i]));
            bits.ecrire_huffman(code, largeur);
            if i + 3 <= donnees.len() {
                vues.entry([donnees[i], donnees[i + 1], donnees[i + 2]]).or_default().push(i);
            }
            i += 1;
        }
    }

    let (fin, largeur_fin) = code_fixe(256);
    bits.ecrire_huffman(fin, largeur_fin);
    bits.terminer()
}

fn morceau(nom: &[u8; 4], corps: &[u8]) -> Vec<u8> {
    let mut sortie = Vec::with_capacity(corps.len() + 12);
    sortie.extend_from_slice(&(corps.len() as u32).to_be_bytes());
    sortie.extend_from_slice(nom);
    sortie.extend_from_slice(corps);
    let mut a_signer = Vec::with_capacity(corps.len() + 4);
    a_signer.extend_from_slice(nom);
    a_signer.extend_from_slice(corps);
    sortie.extend_from_slice(&crc32(&a_signer).to_be_bytes());
    sortie
}

/// Encode une image RVB (trois octets par pixel) en PNG.
///
/// ⚠ Chaque ligne d'un PNG est précédée d'un octet de filtre. On emploie le filtre 1 (« Sub » :
/// chaque octet moins celui de trois rangs avant), qui transforme un aplat de couleur en une
/// longue suite de zéros — précisément ce que LZ77 compresse le mieux. Sur une image de jeu voxel,
/// c'est l'écart entre un fichier utile et un fichier qu'on n'ouvre jamais.
pub fn encoder(largeur: u32, hauteur: u32, rvb: &[u8]) -> Result<Vec<u8>, String> {
    let attendu = (largeur as usize) * (hauteur as usize) * 3;
    if rvb.len() != attendu {
        return Err(format!(
            "l'image annonce {largeur}x{hauteur} soit {attendu} octets, {} recus",
            rvb.len()
        ));
    }

    let l = largeur as usize;
    let mut brut = Vec::with_capacity(attendu + hauteur as usize);
    for y in 0..hauteur as usize {
        brut.push(1u8); // filtre « Sub »
        let ligne = &rvb[y * l * 3..(y + 1) * l * 3];
        for x in 0..ligne.len() {
            let gauche = if x >= 3 { ligne[x - 3] } else { 0 };
            brut.push(ligne[x].wrapping_sub(gauche));
        }
    }

    let mut zlib = vec![0x78, 0x01]; // méthode deflate, fenêtre 32 Ko, sans dictionnaire
    zlib.extend_from_slice(&deflate(&brut));
    zlib.extend_from_slice(&adler32(&brut).to_be_bytes());

    let mut entete = Vec::with_capacity(13);
    entete.extend_from_slice(&largeur.to_be_bytes());
    entete.extend_from_slice(&hauteur.to_be_bytes());
    entete.extend_from_slice(&[8, 2, 0, 0, 0]); // 8 bits, couleur RVB, sans entrelacement

    let mut png = Vec::with_capacity(zlib.len() + 64);
    png.extend_from_slice(&[0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1A, b'\n']);
    png.extend_from_slice(&morceau(b"IHDR", &entete));
    png.extend_from_slice(&morceau(b"IDAT", &zlib));
    png.extend_from_slice(&morceau(b"IEND", &[]));
    Ok(png)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Décompresse un flux `deflate` à arbre fixe — **réservé aux tests**.
    ///
    /// ⚠ Il existe pour une seule raison : *un encodeur ne se prouve pas en regardant sa sortie.*
    /// Vérifier qu'un fichier commence par la signature PNG, ou qu'il fait la bonne taille, ce
    /// sont exactement les sondes qui mentent — elles répondraient pareil si le flux était
    /// illisible. Seul un aller-retour tranche.
    fn inflate_fixe(flux: &[u8]) -> Vec<u8> {
        let mut sortie = Vec::new();
        let mut position = 0usize; // en bits

        let lire = |position: &mut usize, largeur: u32| -> u32 {
            let mut valeur = 0u32;
            for i in 0..largeur {
                let octet = flux[*position / 8];
                let bit = (octet >> (*position % 8)) & 1;
                valeur |= u32::from(bit) << i;
                *position += 1;
            }
            valeur
        };
        let lire_huffman = |position: &mut usize, largeur: u32| -> u32 {
            let mut valeur = 0u32;
            for _ in 0..largeur {
                let octet = flux[*position / 8];
                let bit = (octet >> (*position % 8)) & 1;
                valeur = (valeur << 1) | u32::from(bit);
                *position += 1;
            }
            valeur
        };

        let _dernier = lire(&mut position, 1);
        let methode = lire(&mut position, 2);
        assert_eq!(methode, 1, "ce decodeur ne lit que l'arbre fixe");

        loop {
            // L'arbre fixe se décode par longueur croissante : 7 bits, puis 8, puis 9.
            let sept = lire_huffman(&mut position, 7);
            let symbole: u16 = if sept <= 0x17 {
                (256 + sept) as u16
            } else {
                let huit = (sept << 1) | lire_huffman(&mut position, 1);
                if (0x30..=0xBF).contains(&huit) {
                    (huit - 0x30) as u16
                } else if (0xC0..=0xC7).contains(&huit) {
                    (280 + huit - 0xC0) as u16
                } else {
                    let neuf = (huit << 1) | lire_huffman(&mut position, 1);
                    (144 + neuf - 0x190) as u16
                }
            };

            if symbole == 256 {
                break;
            }
            if symbole < 256 {
                sortie.push(symbole as u8);
                continue;
            }

            let (_, base_l, extra_l) =
                *LONGUEURS.iter().find(|(s, _, _)| *s == symbole).expect("symbole de longueur");
            let longueur = usize::from(base_l) + lire(&mut position, extra_l) as usize;

            let sym_d = lire_huffman(&mut position, 5) as u16;
            let (_, base_d, extra_d) =
                *DISTANCES.iter().find(|(s, _, _)| *s == sym_d).expect("symbole de distance");
            let distance = base_d as usize + lire(&mut position, extra_d) as usize;

            let depart = sortie.len() - distance;
            for k in 0..longueur {
                let octet = sortie[depart + k];
                sortie.push(octet);
            }
        }
        sortie
    }

    /// ⭐ La preuve qui compte : ce qui est comprimé doit se relire **octet pour octet**.
    #[test]
    fn ce_qui_est_comprime_se_relit_a_l_identique() {
        let cas: [&[u8]; 6] = [
            b"",
            b"a",
            b"abc",
            b"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            b"le meme motif, le meme motif, le meme motif, le meme motif",
            // Des octets au-dessus de 143 : ils empruntent le code a NEUF bits, un chemin que les
            // trois premiers cas ne touchent jamais.
            &[200, 201, 202, 200, 201, 202, 200, 201, 202, 255, 0, 255, 0],
        ];
        for donnees in cas {
            let comprime = deflate(donnees);
            assert_eq!(
                inflate_fixe(&comprime),
                donnees,
                "aller-retour rompu sur {} octets",
                donnees.len()
            );
        }
    }

    /// Une image entière, avec ses filtres de ligne, doit revenir intacte.
    #[test]
    fn une_image_entiere_survit_a_l_aller_retour() {
        let (largeur, hauteur) = (17u32, 11u32);
        let mut rvb = Vec::new();
        for y in 0..hauteur {
            for x in 0..largeur {
                // Des aplats coupés d'un bord franc : ce que produit un rendu voxel.
                let bloc = if x < 8 { 40u8 } else { 200u8 };
                rvb.extend_from_slice(&[bloc, (y * 7) as u8, bloc.wrapping_add(30)]);
            }
        }

        let png = encoder(largeur, hauteur, &rvb).expect("l'encodage doit reussir");
        assert_eq!(&png[..8], &[0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1A, b'\n']);

        // On retrouve le flux zlib et on le décomprime, puis on défait les filtres de ligne.
        let debut = png
            .windows(4)
            .position(|f| f == b"IDAT")
            .expect("le morceau de donnees doit exister")
            + 4;
        let taille = u32::from_be_bytes([png[debut - 8], png[debut - 7], png[debut - 6], png[debut - 5]])
            as usize;
        let zlib = &png[debut..debut + taille];
        let brut = inflate_fixe(&zlib[2..zlib.len() - 4]);

        let l = largeur as usize * 3;
        let mut relu = Vec::with_capacity(rvb.len());
        for y in 0..hauteur as usize {
            let ligne = &brut[y * (l + 1)..(y + 1) * (l + 1)];
            assert_eq!(ligne[0], 1, "le filtre annonce doit etre celui qu'on a pose");
            let mut defiltree = vec![0u8; l];
            for x in 0..l {
                let gauche = if x >= 3 { defiltree[x - 3] } else { 0 };
                defiltree[x] = ligne[1 + x].wrapping_add(gauche);
            }
            relu.extend_from_slice(&defiltree);
        }
        assert_eq!(relu, rvb, "l'image relue doit etre exactement celle qu'on a donnee");

        // ⚠ La somme de contrôle porte sur les données AVANT compression : la vérifier ici prouve
        // qu'on n'a pas simplement écrit un nombre plausible.
        let adler = u32::from_be_bytes([
            zlib[zlib.len() - 4],
            zlib[zlib.len() - 3],
            zlib[zlib.len() - 2],
            zlib[zlib.len() - 1],
        ]);
        assert_eq!(adler, adler32(&brut), "somme de controle zlib");
    }

    /// Une taille annoncée qui ne correspond pas doit être refusée, pas devinée.
    #[test]
    fn une_image_de_taille_incoherente_est_refusee() {
        assert!(encoder(4, 4, &[0u8; 10]).is_err());
        assert!(encoder(2, 2, &[0u8; 12]).is_ok());
    }

    /// ⚠⚠ LA GARDE QUI REND LE RETOUR DU PYTHON IMPOSSIBLE.
    ///
    /// La règle « QUE du Rust, aucun autre langage » a été enfreinte pendant des mois **dans le
    /// binaire livré**, et rien ne le disait : le code compilait, les tests passaient, et la
    /// capture marchait sur la machine de développement — la seule où `python3` est installé.
    ///
    /// *Écrire la règle est la moitié du travail ; la rendre inatteignable est l'autre.* Ce test
    /// parcourt tout le moteur et refuse le lancement d'un interpréteur tiers.
    #[test]
    fn le_moteur_ne_lance_aucun_interprete_etranger() {
        let fichiers: [(&str, &str); 4] = [
            ("core/engine.rs", include_str!("../core/engine.rs")),
            ("core/gpu_context.rs", include_str!("../core/gpu_context.rs")),
            ("image/png.rs", include_str!("png.rs")),
            ("render/cadre.rs", include_str!("../render/cadre.rs")),
        ];
        let interdits = ["python", "python3", "node", "perl", "ruby", "sh", "bash"];

        let mut coupables = Vec::new();
        for (nom, source) in fichiers {
            for (numero, ligne) in source.lines().enumerate() {
                // Seul le CODE compte : ce fichier PARLE de python dans ses commentaires, et une
                // sonde qui compterait son propre vocabulaire ne mesurerait rien.
                let code = ligne.split("//").next().unwrap_or("");
                if !code.contains("Command::new") {
                    continue;
                }
                if interdits.iter().any(|i| code.contains(&format!("\"{i}\""))) {
                    coupables.push(format!("{nom} ligne {}", numero + 1));
                }
            }
        }

        assert!(
            coupables.is_empty(),
            "le moteur lance un interprete etranger, ce que le projet interdit sans exception :\n               {}\nUn outil ecrit dans un autre langage n'existe pas sur la machine du joueur.",
            coupables.join("\n  ")
        );
    }

    /// Le CRC doit être celui de la spécification, pas une somme quelconque.
    #[test]
    fn le_crc_est_celui_de_la_specification() {
        // ⚠ Ces deux valeurs se REDEMONTRENT, elles ne se citent pas de memoire. `0xAE426082` est
        // le CRC de « IEND », visible en fin de tout PNG du monde ; l'adler de « abc » se calcule
        // en trois lignes : a = 1+97+98+99 = 295, b = 98+196+295 = 589, donc (589 << 16) | 295.
        //
        // *La premiere ecriture de ce test citait `0x02470805`, de memoire, et c'etait faux.* Le
        // code, lui, etait juste. Une assertion fausse aurait pu faire « corriger » un encodeur
        // correct — c'est exactement pourquoi un banc qui refuse de confirmer est une INFORMATION
        // avant d'etre un test a reparer.
        assert_eq!(crc32(b"IEND"), 0xAE42_6082);
        assert_eq!(adler32(b"abc"), 0x024D_0127);
    }
}

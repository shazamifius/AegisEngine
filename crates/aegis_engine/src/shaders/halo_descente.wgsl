// ── LE HALO, LA DESCENTE : UNE OCTAVE DE PLUS A CHAQUE MARCHE ───────────────────────────────
//
// D'un niveau vers le suivant, deux fois plus petit. Repetee jusqu'a ce qu'un niveau fasse moins
// de huit pixels — c'est ce qui donne au halo un rayon qui est une fraction fixe de l'ecran,
// identique en 1080p et sur un casque, sans qu'aucun chiffre ne le decide.
//
// Conception complete en tete de `halo.wgsl`.

//!inclure halo

@fragment
fn fs_main(in: SortiePleinEcran) -> @location(0) vec4<f32> {
    // ⚠ `false` : plus aucun seuil. Ce qu'on lit ici EST deja le debordement, retenu une fois par
    // `halo_extraction`. Seuiller a nouveau retirerait le blanc affichable a chaque niveau, et le
    // halo s'eteindrait par le milieu — un defaut qui ressemblerait a un reglage trop faible.
    return vec4<f32>(reduire(in.uv, false), 1.0);
}

// ── LE HALO, PREMIERE MARCHE : RETENIR CE QUI DEBORDE ───────────────────────────────────────
//
// De la scene en pleine resolution vers le premier niveau, en demi. Deux gestes a la fois : on
// retient ce qui depasse le blanc affichable, et on reduit de moitie.
//
// Les faire ensemble n'est pas une economie de fichier : c'est ce qui evite d'ecrire une image
// pleine resolution de plus, la ou la bande passante est la ressource rare. Toute la conception
// (le seuil, les poids, le rayon) est expliquee en tete de `halo.wgsl`.

//!inclure halo

@fragment
fn fs_main(in: SortiePleinEcran) -> @location(0) vec4<f32> {
    // `true` : c'est la SEULE passe qui seuille. Les niveaux suivants ne portent deja plus que du
    // debordement — leur en retirer un second serait soustraire deux fois la meme chose.
    return vec4<f32>(reduire(in.uv, true), 1.0);
}

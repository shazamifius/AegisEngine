// ── UNE COPIE PLEIN ECRAN, ET C'EST LE MELANGE QUI TRAVAILLE ────────────────────────────────
//
// Ce shader ne fait rien d'autre que relire une image. Toute la chaine d'occlusion tient dans son
// MELANGE : monte en `Melange::Multiplicatif` il multiplie sa cible par ce qu'il lit, monte en
// `Melange::Soustractif` il l'en retire.
//
// ⭐ C'est ce qui evite un shader qui prendrait deux images pour les combiner — donc un second
// agencement de descripteurs, donc une seconde reserve, donc deux choses a tenir d'accord. La
// carte sait deja combiner ce qu'on ecrit avec ce qui est la ; il suffisait de le lui demander.

//!inclure plein_ecran

@fragment
fn fs_main(in: SortiePleinEcran) -> @location(0) vec4<f32> {
    return textureSample(source, echantillonneur, in.uv);
}

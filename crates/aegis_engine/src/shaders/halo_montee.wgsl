// ── LE HALO, LA MONTEE : SUPERPOSER LES ECHELLES ────────────────────────────────────────────
//
// D'un niveau vers le precedent, deux fois plus grand, en se MELANGEANT a ce qui s'y trouve deja.
//
// ⭐ Le melange moitie-moitie ne se fait PAS ici mais dans le pipeline (`Melange::Moitie`) : la
// carte sait combiner ce que le shader ecrit avec ce qui est deja dans l'image, sans avoir a le
// relire. C'est une texture de moins a lire par pixel, et c'est ce qui donne les poids 1/2, 1/4,
// 1/8… dont la somme fait exactement 1.
//
// ⚠ La toute derniere montee, elle, s'ajoute a la SCENE en additif pur — on ne melange pas la
// lumiere du monde a moitie avec son propre halo, on la lui ajoute. Deux melanges differents pour
// le meme shader : c'est le pipeline qui en decide, pas ce fichier.
//
// Conception complete en tete de `halo.wgsl`.

//!inclure halo

@fragment
fn fs_main(in: SortiePleinEcran) -> @location(0) vec4<f32> {
    return vec4<f32>(agrandir(in.uv), 1.0);
}

// ── DE LA LUMIERE AU PIXEL — LA DERNIERE PASSE, ET LA SEULE QUI COURBE ──────────────────────
//
// ## Ce qui a change le 30 aout 2026, et pourquoi ca depasse largement le halo
//
// Jusqu'ici, CHAQUE shader finissait par `presenter(...)` et ecrivait directement dans l'image
// montree a l'ecran. Deux consequences, et la seconde n'etait pas cherchee :
//
//  1. **La scene s'arretait a 1,0**, le blanc de l'ecran. Un mur blanc et le soleil y avaient la
//     meme valeur. Aucun effet capable de distinguer « clair » de « LUMINEUX » n'etait donc
//     possible — pas par manque de finition, par impossibilite.
//  2. **La courbe de tonalite vivait a DEUX endroits** (le fond, les objets), et un troisieme
//     oubli restait possible a chaque nouveau shader. Un test montait la garde ; une garde qui
//     surveille une duplication est l'aveu qu'elle existe.
//
// La scene se dessine maintenant en lumiere brute, et **ce fichier est le seul endroit du moteur
// ou une courbe de tonalite s'applique**. Le fond, les objets, les particules et le halo passent
// tous par la meme, non pas parce qu'on les a regles pareil, mais parce qu'il n'y en a qu'une.
// *La question « ai-je oublie la courbe quelque part » a cesse d'avoir un sens.*
//
// ⚠ Le HUD ne passe PAS par ici : il se dessine APRES, directement a l'ecran. C'est voulu et
// c'est la seule chose juste — une interface n'est pas dans la scene, elle ne recoit aucune
// lumiere, et un texte blanc courbe par une exposition sortirait gris.

//!inclure commun
//!inclure plein_ecran

@fragment
fn fs_main(in: SortiePleinEcran) -> @location(0) vec4<f32> {
    // La scene telle qu'elle a ete eclairee — et, si le halo a tourne, avec sa diffusion deja
    // ajoutee dedans. Rien ici ne sait lequel des deux cas s'applique, et c'est bien ainsi.
    let lumiere = textureSample(source, echantillonneur, in.uv).rgb;

    // ⚠ Le resultat est LINEAIRE : c'est le format de la surface de presentation qui encode la
    // gamma. Un `pow(x, 1/2.2)` ici la coderait une seconde fois — la faute exacte du 29 aout,
    // qui delavait toute l'image et qu'un commentaire promettait pourtant de ne pas commettre.
    return vec4<f32>(presenter(lumiere), 1.0);
}

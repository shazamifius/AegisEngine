// ── LE FOND : CE QU'ON VOIT LA OU RIEN N'EST DESSINE ────────────────────────────────────────
//
// ## Ce que ce fichier faisait avant le 29 aout 2026, et pourquoi c'etait le vrai defaut
//
// Il peignait un degrade « blanc pur studio » ecrit en dur — 0,97 en haut, un bleu glacial en
// bas, plus une bande d'ombre et un projecteur simules — **sans jamais lire l'eclairage de la
// scene**. Pendant ce temps les objets etaient eclairs par une ambiante a 0,17.
//
// Le resultat se decrivait exactement comme l'utilisateur l'a decrit, en deux phrases qui
// semblaient se contredire : *« l'univers est super sombre »* ET *« le plan est toujours aussi
// blanc »*. Les deux etaient vraies. L'œil s'adapte au blanc dominant, et tout le reste parait
// terne. **Ce n'etait pas une question de gout : c'etaient deux mondes dans la meme image.**
//
// Cinq couleurs vivaient ici, donc cinq decisions d'artiste gravees dans le moteur. Elles ont
// disparu — pas retrecies, pas deplacees dans une constante mieux nommee : **il n'y en a plus.**
//
// ## Ce qu'il fait maintenant
//
// Le fond, c'est le ciel regarde en face. Les ombres, c'est ce meme ciel integre sur une
// hemisphere. Une seule fonction repond donc aux deux — `ambiance_hemispherique`, dans
// `commun.wgsl` — appelee ici avec la direction du regard, et la avec la normale. Le fond et les
// objets ne peuvent plus se contredire : ce n'est pas qu'on les a regles pareil, c'est le meme
// calcul, avec les memes couleurs, posees par le JEU.
//
// ⚠ Ce shader n'a aucun sommet a transformer (trois points couvrent l'ecran). C'est pourquoi le
// cadre porte `inv_view_proj` : sans elle, on ne saurait pas quelle direction du monde chaque
// pixel regarde, et il ne resterait qu'a inventer un degrade — c'est-a-dire choisir une couleur.

//!inclure commun

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) ndc: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var out: VertexOutput;
    // Un seul triangle plus grand que l'ecran, plutot que deux : moins de sommets, et surtout
    // aucune diagonale au milieu de l'image ou les deux moities pourraient s'ombrer differemment.
    var positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>( 3.0, -1.0),
        vec2<f32>(-1.0,  3.0)
    );

    let p = positions[vertex_index];
    out.clip_position = vec4<f32>(p, 0.0, 1.0);

    // ⚠⚠ LE MEME PIEGE QUE LA CARTE D'OMBRE, ET IL COUTE AUSSI CHER.
    // naga compile avec ADJUST_COORDINATE_SPACE, qui inverse le Y de la position de clip EN
    // SORTIE DE VERTEX. Ce que le rasteriseur voit n'est donc pas `p`, mais `p` avec un Y oppose.
    // Comme ce Y sert ensuite a retrouver une direction dans le monde, l'oublier retournerait le
    // ciel et le sol — sans qu'aucune ligne ne paraisse fausse.
    out.ndc = vec2<f32>(p.x, -p.y);
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Du point de l'ecran a la direction que le regard suit dans le monde. Le plan lointain
    // (z = 1) suffit : seule la direction compte, la distance n'a pas de sens pour un ciel.
    let lointain = cadre.inv_view_proj * vec4<f32>(in.ndc, 1.0, 1.0);
    let direction = normalize(lointain.xyz / lointain.w - cadre.camera_et_compte.xyz);

    // ⚠ Aucune courbe de tonalite ici, et c'est le changement du 30 aout : ce shader ecrit de la
    // LUMIERE, pas une couleur d'ecran. La courbe est appliquee une seule fois, tout a la fin,
    // dans `composition.wgsl`. Le fond et les objets ne peuvent donc plus vivre dans deux espaces
    // de tons differents — non pas parce qu'on les a regles pareil, mais parce qu'il n'y a plus
    // qu'un seul endroit ou la question se pose.
    return vec4<f32>(ambiance_hemispherique(direction), 1.0);
}

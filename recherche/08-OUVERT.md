# 08 — Ce qui est ouvert : les carrefours, les trous, et le programme

> **C'est la page la plus utile du dossier**, et la seule qui compte vraiment si vous lisez pour
> contribuer ou pour contredire. Elle liste ce qui n'est **pas** décidé, ce qui n'est **pas** mesuré,
> ce qui n'est **pas** lu — et dans quel ordre attaquer.

---

## 1. Les carrefours non tranchés

*Chacun avec ses options, son coût, et **ce qu'il faudrait mesurer pour trancher**. Un carrefour sans
critère de décision n'est pas un carrefour : c'est une hésitation.*

### ⭐⭐ C1 — Sur quelle structure la lumière voyage-t-elle ? · risque 🔴 maximal

**Rien d'autre ne peut être décidé avant.** Cette décision commande la lumière indirecte, la texture,
l'ombrage stéréo et le découplage temporel **d'un seul coup**.

| Option | Pour | Contre |
|---|---|---|
| **Surfels** | Quatre équipes indépendantes y convergent, dont une **en production sur mobile**. Pas d'UV, supporte le skinning, empreinte prévisible | Un nuage sans topologie : recherche spatiale pour trouver les voisins, coutures d'interpolation, gestion naissance/mort |
| **Micro-triangles barycentriques** | Connectivité gratuite, interpolation continue, suivent la déformation sans être re-semés, **moins de mémoire par échantillon** (ni position ni normale à stocker) | Aucun précédent **barycentrique**. ⚠ Un plancher structurel : jamais moins d'échantillons que le maillage n'a de triangles |

### ⚠⚠ DEUX CORRECTIONS DU 5 SEPTEMBRE 2026 — cette section affirmait deux choses fausses

**1. « Personne ne l'a fait » était faux.** *Radiance Caching with On-Surface Caches* (HPG 2024,
TU Graz + Huawei) fait tenir la radiance **exclusivement sur la surface**, en temps réel, mesuré et
comparé à Lumen et AMD-GI1.0. Sa paramétrisation est un **atlas d'îlots auto-générés**, pas des
coordonnées barycentriques — ce n'est donc pas exactement cette voie, mais c'en est le voisin le plus
proche, et il est chiffré. *L'absence de précédent n'était pas une propriété du sujet : c'était une
propriété de notre recherche.*

**2. La mesure demandée était mal posée**, et elle aurait envoyé une session sur une fausse piste.
Elle supposait que les micro-triangles se passent d'accélération spatiale. **Ils ne le peuvent pas :**
la connectivité d'un maillage donne les VOISINS d'un échantillon sur la surface ; elle ne dit rien de
ce qu'un rayon frappe à l'autre bout de la scène. Un rayon traverse du vide, et le vide n'a pas de
topologie. *Formulée ainsi, la mesure aurait rendu « micro-triangles = infiniment cher » et tranché
pour les surfels **pour une raison fausse**.*

> ### Ce sont DEUX AXES INDÉPENDANTS, pas deux options concurrentes
>
> - **Le support de radiance** — où la lumière est stockée, interpolée, suivie sous déformation ;
> - **La structure de visibilité** — comment un rayon trouve ce qu'il touche.
>
> **Ce dossier en portait déjà la preuve sans la voir** : son propre tableau (§2 de
> [04](04-LUMIERE.md)) donne Lumen pour « champs de distance **+** cache de surface » et Split RC
> pour « table de hachage **+** une BVH ». Deux structures, écrites noir sur blanc, présentées comme
> une seule décision.
>
> **Et un troisième axe manquait : le SCHÉMA DE TRANSPORT** — comment l'énergie passe d'un point du
> support à un autre. Les cascades de radiance en sont un. *Il a été tranché avant le support, alors
> que `0.a` est annoncé comme « rien d'autre ne peut être décidé avant ».*

> **La mesure qui décide vraiment, re-posée :**
> **À structure de visibilité IDENTIQUE, quel support touche le moins d'octets par échantillon de
> radiance mis à jour — et comment ce chiffre varie-t-il avec la cohérence du motif d'accès ?**

### ⭐ Ce que la mesure du 5 septembre a effectivement rendu

*Banc : `cargo run --release -p aegis_engine --example topologie --no-default-features`.*

| Ce qui a été mesuré | Résultat | Nature |
|---|---|---|
| Adjacence exploitable sur un `.glb` Blender réel | **100 %**, zéro non-manifold, zéro triangle dégénéré | `MESURÉ` — ⚠ sur une topologie **quad triangulée**, c'est-à-dire le meilleur cas possible : c'est un **plafond**, pas une moyenne |
| Sommets dupliqués par l'exportateur | **73,5 %** — la lecture par indices bruts ne voit que **37 %** de l'adjacence réelle | `MESURÉ` — la connectivité n'est gratuite qu'**après** une soudure au chargement |
| Dispersion des aires de triangles | **20 610 ×** entre le plus grand et le plus petit (le corpus annonçait « mille à un ») | `MESURÉ` |
| Coût mémoire comparé, radiance scalaire | **+29 à +42 %** en faveur du micro-triangle à 1–5 cm · **égalité** à 20 cm · **−39 %** à 30 cm | `CALCULÉ` |

> ### ⛔ Le résultat NÉGATIF, et il est structurel
> **Un micro-maillage ne peut jamais porter moins d'échantillons que le maillage n'a de triangles.**
> C'est un plancher ; un surfel n'en a pas — un seul disque de 30 cm recouvre dix triangles fins.
> S'y ajoute que **la subdivision ne progresse que par puissances de 4** : elle dépasse la densité
> visée au lieu de l'atteindre, mesuré à **≈ 2 × d'échantillons en trop**. Les 6 octets contre 16 en
> rapportent 2,67 — il reste ~30 % de gain net, **et il s'évapore dès que la lumière visée est plus
> grossière que la géométrie.**
>
> **Donc `C1` ne se tranche pas « en général ».** Le bon support dépend du rapport entre la finesse
> de la lumière voulue et la finesse du maillage : **plus fine que le maillage → micro-triangles ;
> plus grossière → surfels.** Et cette finesse est un chiffre que personne n'a encore.

⚠ **Et une limite du chiffrage ci-dessus, à ne pas perdre :** le rapport 6/16 suppose une radiance
**scalaire**. `On-Surface Caches` mesure **1 134 octets par entrée** parce qu'il stocke un hémisphère
directionnel 8×8 — ce qu'exigent les reflets et les cascades. Dans ce cas les deux familles paient le
même hémisphère, l'avantage mémoire disparaît, et **le sur-échantillonnage de 2 × fait perdre la voie
barycentrique**. Il ne lui resterait alors que la connectivité.

Détail complet : [04 — La lumière indirecte](04-LUMIERE.md) §4.

### C2 — S'appuyer sur la reprojection du casque ? · risque 🟠 élevé

Le rendu à demi-cadence avec synthèse par le casque **double le budget** : 13,9 → ≈ 27,8 ms. C'est de
loin le levier le plus puissant du dossier.

⚠ **Mais c'est une reprojection temporelle**, la famille même que ce dossier écarte
([02 — Le budget](02-BUDGET.md) §6). La distinction est réelle : elle est appliquée **par le casque,
sur l'image finie**, avec les vecteurs de mouvement qu'on lui fournit — ce n'est pas un débruiteur qui
reconstruit un signal bruité.

> **Ce qu'il faut d'abord :** `VK_KHR_multiview` (prérequis absolu, **inexistant ici**), puis une
> mesure d'artefacts en conditions réelles. **Et ça demande un casque.** *C'est donc un carrefour qui
> ne pourra jamais être tranché par la mesure dans ce projet — seulement par un utilisateur.*

### C3 — Le format de la transmittance dans les cascades · risque 🟡 moyen

Un $\beta$ spectral ferait entrer l'absorption dans le socle et refermerait quatre lignes de l'étalon
d'un coup ([04 — La lumière indirecte](04-LUMIERE.md) §6). Il coûte de la
mémoire :

| | $\beta$ binaire | $\beta$ RVB 8 bits | $\beta$ RVB 16 bits |
|---|---|---|---|
| bits par entrée | 49 | 72 | 96 |
| rapport | ×1,00 | **×1,47** | **×1,96** |

> **À mesurer :** le coût réel de la fusion en octets par image aux trois formats ; et si 8 bits par
> canal suffisent — la transmittance est bornée dans $[0,1]$ et se lit perceptuellement, donc
> probablement oui.

### C4 — Budget global redistribuable, ou budget par passe ? · risque 🟡 moyen

La formulation lagrangienne de [06 — L'adaptativité](06-ADAPTATIVITE.md) suppose un budget **global**,
et son optimum égalise le rendement marginal de toutes les briques. Elle exige d'estimer
$\partial U/\partial q_i$ — la qualité perçue gagnée par unité de poignée — **et personne ne sait
faire ça objectivement**.

Un budget **par passe** ne demande aucune estimation de $U$, se raisonne facilement, et est
**strictement moins bon**.

> **Ce qui pourrait trancher :** une expérience où l'on fait régler les poignées à la main sur trois
> scènes de référence, et où l'on compare le réglage humain à ce qu'un budget par passe aurait donné.
> *Si l'écart est faible, la simplicité gagne.*

### C5 — Le langage de shaders · risque 🟢 faible, mais il se referme avec le temps

Aujourd'hui : **WGSL compilé en SPIR-V à la construction, sans aucune dépendance à l'exécution.**
11 shaders réels.

| Piste | Ce que ça donnerait | Verdict de travail |
|---|---|---|
| **Rester en WGSL** | Ce qu'on a | ✅ **défaut à conserver.** ⚠ Piège connu : le compilateur **retourne l'axe Y**, et rien dans le code ne le dit — c'est ce qui a sorti le HUD à l'envers avec onze tests verts |
| **Slang** | Modules, génériques, réflexion, dérivation automatique — conçu **pour** le problème des permutations | ⚠ une dépendance de plus, et un compilateur C++ |
| **rust-gpu** | Shaders **en Rust** | 🟡 séduisant pour un projet tout-Rust, mais c'est une chaîne de compilation entière à adopter |
| **Écrire son propre langage** | Cohérent avec la doctrine | ⛔ le plus gros chantier du dossier, **et il ne produit aucune image** |

> ⚠⚠ **Et le mur qui vient, quel que soit le choix : les permutations.** Quatre interrupteurs
> indépendants font **16** variantes ; un éditeur de graphes est un **multiplicateur par-dessus** —
> 100 permutations d'éclairage × 100 graphes = **10 000 shaders** à compiler, stocker et charger. Sur
> une machine où la mémoire est rare, ce n'est pas un inconfort de compilation : c'est un coût de
> démarrage et de premier affichage saccadé chez celui qui joue. **Toute conception de système de
> shaders doit dire, dès sa première page, comment elle borne ses permutations.**

### C6 — Les inclusions fines · risque 🟡 moyen

Un volume cuit ne peut pas porter des bulles fines : il faudrait ≈ 27 Go
([03 — La matière](03-MATIERE.md) §6). Trois pistes, **aucune
instruite** : volume creux · détail procédural par-dessus une structure grossière · inclusions
instanciées.

*C'est une propriété du choix d'architecture, pas un défaut d'implémentation : une grille régulière
paie **partout** la finesse dont elle n'a besoin **que par endroits**.*

### C7 — Une version du moteur, ou plusieurs ? · risque 🟢 faible

La doctrine penche pour **une seule**, qui s'adapte. *Une seconde version est une version que
personne ne teste.* ⚠ Mais la taille à l'installation est un vrai sujet, et il est lié à
l'adaptativité : **un moteur qui s'adapte ne doit pas embarquer $N$ jeux de données**, sinon il paie
en octets ce qu'il gagne en souplesse. *La bonne réponse est probablement de faire varier des
**nombres**, pas des assets — la même ligne rouge que pour les algorithmes.*

---

## 2. Les mesures qui manquent

*Rangées par ce qu'elles débloquent, pas par difficulté.*

| # | Ce qu'il faut mesurer | Ce que ça débloque | Faisable ici ? |
|---|---|---|---|
| ~~M1~~ | ~~Le parcours entre micro-triangles **sans** structure spatiale, contre surfels **avec**~~ | — | ⛔ **MAL POSÉE, retirée le 5 sept. 2026** — voir `C1` : elle supposait qu'un support de radiance puisse se passer de visibilité |
| **M1a** | **À visibilité identique**, les octets touchés par échantillon mis à jour, sous un motif d'accès cohérent puis incohérent | **C1** | ✅ oui, banc processeur |
| **M1b** | La fraction d'arêtes 2-manifold sur du contenu **sale** — importé, décimé, scanné | `C1` : l'argument « connectivité gratuite » ne vaut aujourd'hui que sur un maillage propre | 🟡 il manque le contenu, pas l'outil : le banc `topologie` existe |
| **M1c** | La finesse à laquelle la lumière indirecte doit vivre, lue sur une **vraie scène** | **C1 en dépend entièrement** — c'est le chiffre qui manque, et il ne se trouve pas sur le papier | ✅ oui, depuis le 5 sept. : le moteur charge et rastérise une scène Blender complète |
| **M2** | Le coût de la passe qui rastérise une **scène complète** dans les deux cartes | Le chaînon manquant entre la physique et une image de jeu | ✅ oui |
| **M3** | Le coût d'un $\beta$ spectral en octets par image | **C3** | ✅ oui, dès qu'une cascade existe |
| **M4** | La cause du **pire cas à 10× la moyenne** | L'asservissement (§C4) : sans elle, on ne sait pas ce qu'on asservit | ✅ oui, et c'est **prioritaire** |
| **M5** | La version Vulkan réelle du Quest 2 | Si c'est 1.1, le repli est `VK_KHR_dynamic_rendering` en extension | 🟡 par la documentation constructeur, pas par la mesure |
| **M6** | Le coût GPU réel sur un mobile | Tout le budget | ⛔ **impossible sans un `.apk`** — le terminal Android n'a pas accès au pilote GPU |
| **M7** | Quoi que ce soit sur Quest 2 | Tout | ⛔ **jamais** |

> ⭐ **M6 change de statut :** l'`.apk` n'est plus « la voie vers une mesure GPU mobile », c'est **la
> seule voie vers une mesure GPU mobile, pour toujours**. Une vraie application Android a accès au
> pilote ; un terminal, non. *Ce n'est plus un chantier de confort : c'est le seul instrument que ce
> projet pourra jamais braquer sur un GPU mobile.*

---

## 3. Les documents non lus, et ce qu'ils coûtent

| Document | État | Pourquoi ça compte |
|---|---|---|
| **Les planches de *Love and Deepspace*** (GDC 2026) | 🟡 **toujours introuvables** (la vidéo est derrière le GDC Vault), mais la **description officielle de la session** a été lue le 5 sept. 2026 | ⭐⭐⭐ Elle donne les trois points techniques, et **deux sont des aveux de coût** : *« comment faire du lancer de rayons logiciel **sur eux** »* — donc chez eux la visibilité **EST** le support — et *« comment **résoudre les trous entre surfels** »*, ce qu'un maillage n'a pas. Leur déclencheur est le nôtre : *« après l'introduction du contenu créé par les joueurs, le pré-calcul statique ne suivait plus »* |
| **Radiance Caching with On-Surface Caches** (HPG 2024) | ✅ **lu en entier**, archivé | ⭐⭐⭐ **Le précédent que ce dossier déclarait inexistant.** Radiance exclusivement sur la surface, temps réel. **1 134 o par entrée** (hémisphère directionnel 8×8), **1,45 à 2,64 ms** sur RTX 3090 Ti en 1080p **un œil avec RT matériel**, 32 à 456 Mo de caches. ⛔ Transposé à notre budget : **hors budget d'un facteur ~10**, comme ReSTIR |
| **HSGI** (LIGHTSPEED, GDC 2023) · **surfel-maintenance** (m4xc) | ✅ lus, archivés | Ils chiffrent **le coût caché des surfels** : table de hachage multi-niveaux à 3 passes GPU, semis à **~1 ms sur un iGPU de bureau** après optimisation, plus le recyclage. *Un coût que le micro-triangle n'a pas du tout* |
| **Le papier fondateur de Sannikov** | 🟡 référencé, pas lu en entier | Les formules de cascades citées ici viennent d'une **source pédagogique**, pas de l'original. **À revérifier avant d'écrire une ligne de code dessus** |
| **Les notes de HypeHype** (56 Ko) | 🟡 récupérées, non dépouillées | Le patron d'échantillonnage par tuiles à deux étages, et la règle « fp16 partout, écrit dès le départ » |
| **Smolder** — volumétrique, SIGGRAPH 2026 | ⛔ non ouvert | Moins l'effet que **l'intégration** : comment le volumétrique cohabite avec le reste sans devenir un chemin de rendu à part |
| **Htex** (arXiv 2207.05618) | ⛔ non lu | La réponse directe à « en tout-triangle le dépliage est un enfer » |

---

## 4. Le programme, ordonné

**Le principe d'ordre :** *le socle d'abord, en sachant déjà la forme du sommet.* **Et celui du
projet :** *le mur le plus INCERTAIN d'abord, à la plus petite échelle qui peut le révéler.* Un
chantier long mais sans surprise attend ; un chantier qui peut **invalider le plan entier** passe
devant.

```mermaid
flowchart TD
    subgraph E0["ÉTAGE 0 · LE SOCLE"]
        A["<b>0.a</b> trancher la structure<br/>de la lumière — C1<br/>🔴 incertitude maximale"]
        B["<b>0.b</b> une CI<br/>🟢 petite, mais c'est le filet"]
        C["<b>0.c</b> la boucle<br/>d'adaptativité<br/>🟡 moyenne"]
    end
    subgraph E1["ÉTAGE 1 · LE RENDU DE SURFACE"]
        D["<b>1.a</b> la passe qui rastérise<br/>une SCÈNE dans les deux cartes<br/><i>le chaînon manquant</i>"]
        E["<b>1.b</b> LEAN mapping<br/><i>il dort déjà dans le dépôt</i>"]
        F["<b>1.c</b> SSILVB<br/><i>et c'est le C(−1) des cascades</i>"]
    end
    subgraph E2["ÉTAGE 2 · LA LUMIÈRE QUI BOUGE"]
        G["cascades de radiance<br/>sur la structure de 0.a"]
    end
    subgraph E3["ÉTAGE 3 · LA SURFACE SANS UV"]
        H["Htex ou ombrage<br/>en espace texture"]
    end
    subgraph E4["ÉTAGE 4 · LA GÉOMÉTRIE"]
        I["micro-maillages · Loop · QEM"]
    end
    subgraph E5["ÉTAGE 5 · LE SOMMET"]
        J["personnages animés<br/>OpenXR · Android · stéréo"]
    end
    A --> G
    B --> D
    C --> G
    D --> G
    E --> G
    F --> G
    G --> H
    H --> I
    I --> J

    style A fill:#f4f1ea,stroke:#8c3b2e,stroke-width:2px
    style G fill:#f4f1ea,stroke:#7d4b2a,stroke-width:2px
    style J fill:#f4f1ea,stroke:#9c9488,stroke-width:2px
```

### Ce qui est recommandé, et pourquoi

1. **Commencer par 0.a, et le faire SUR LE PAPIER.** C'est la décision la plus lourde, la plus
   incertaine, et **la moins chère à prendre maintenant** — *chercher le mur sur le papier d'abord.*
   Elle se tranche en lisant deux documents et en écrivant **un banc processeur** (M1), pas en
   écrivant un moteur.
2. **Puis 0.b avant 0.c.** La CI est petite, et c'est le seul filet pour tout ce qui suit — *le rendu
   est précisément ce qui casse en silence.*
   ⚠ **Avec un pré-requis qui n'était écrit nulle part :** la suite de tests plantait une fois sur
   quatre sans nommer de coupable. Une CI posée avant ce correctif aurait été **rouge une fois sur
   quatre sans raison**, sur le premier projet du dépôt à en avoir une. **La fiabilité de la suite est
   une condition de possibilité de la CI.**
3. **Et une chose qui devrait peut-être passer avant tout :** *tout ce dossier suppose une scène 3D.
   Ce moteur n'a jamais affiché autre chose qu'une scène simple générée en code.* Charger un vrai
   modèle depuis Blender, l'éclairer, le regarder — **c'est une demi-journée, et ça dirait plus sur
   l'écart réel que dix documents.** Le chargeur existe déjà.

---

## 5. ⚠ Ce qui invaliderait ce plan

*Une feuille de route qui ne dit pas comment elle peut être fausse n'est pas une feuille de route.*

| Si l'on mesurait… | Alors |
|---|---|
| que le parcours entre micro-triangles coûte plus de deux fois celui entre surfels | **C1 se tranche pour les surfels**, contre la préférence affichée — et il faudra le dire |
| que les cascades 3D ne tiennent pas sous 4 ms sur une machine mobile, même en lancer logiciel | **Toute la famille tombe**, et il ne reste que l'éclairage cuit + espace écran. *Ce serait un résultat, pas un échec* |
| que le Quest 2 est en Vulkan 1.1 | Le repli est une extension à demander, **pas une réécriture** — mais il faut le savoir avant, pas après |
| qu'un $\beta$ spectral coûte plus de 50 % de la bande passante de fusion | L'idée de [04](04-LUMIERE.md) §6 tombe, ou se restreint aux cascades basses |
| que le pire cas à 10× la moyenne vient d'une cause structurelle et non d'un artefact de mesure | **L'adaptativité devient beaucoup plus difficile** : on n'asservit pas une distribution à queue lourde comme une distribution ordinaire |

> **Un résultat négatif est un résultat.** Le but n'est pas d'obtenir des chiffres élevés : c'est de
> comprendre ce qui est vrai.

---

## 6. Ce qu'on aimerait qu'on nous dise

Ce dossier est public pour une raison : **une question extérieure compétente est un instrument de
mesure gratuit**, et elle a un avantage qu'aucune relecture interne n'a — *elle ne partage aucun de
nos présupposés, donc elle demande à voir ce qu'on tient pour acquis.*

Trois choses seraient particulièrement utiles :

1. **Un lien vers les planches de *Love and Deepspace*** (GDC 2026, présenté par Arm). C'est le trou
   n° 1 de ce chantier.
2. **Une réfutation chiffrée de [04 §6](04-LUMIERE.md) §6** — le $\beta$
   spectral. Si quelqu'un l'a déjà essayé et que ça ne tient pas, le savoir maintenant vaut mieux que
   de le découvrir dans six mois.
3. **N'importe quelle mesure prise sur un Quest 2.** Ce projet n'en aura jamais.

Et si vous trouvez une erreur dans ce dossier, **elle est probablement réelle**. Les affirmations y
portent leur nature et leur commande de re-vérification exactement pour ça.

---

## Le résumé, en une page

| | État | Prochain geste |
|---|---|---|
| **Physique de la matière** | 4 phénomènes conquis, démontrés contre des vérités analytiques | **M2** — la passe qui les amène dans une image |
| **Lumière indirecte** | ⛔ rien. Une famille éliminée par la mesure, une famille candidate | **C1** sur le papier, puis **M1** |
| **Géométrie** | rendu 100 % triangulaire · ⭐ **une scène Blender complète se charge et se rastérise** depuis le 5 sept. 2026 (`GlbLoader::charger_scene` : toutes les primitives, hiérarchie composée, transformations appliquées, miroirs rétablis) | **M1c** — lire sur elle la finesse à laquelle la lumière doit vivre |
| **Adaptativité** | l'instrument existe et est prouvé · la boucle n'existe pas | **M4** — expliquer le pire cas |
| **VR** | ⛔ **rien.** Pas d'OpenXR, pas de stéréo, pas d'Android | `VK_KHR_multiview`, puis un `.apk` |
| **Outillage** | tests d'image GPU ✅ · CI ⛔ | **0.b** |

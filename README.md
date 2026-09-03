# AegisEngine

Un moteur de rendu 3D écrit à la main en Rust, directement sur Vulkan — sans couche
intermédiaire, sans `wgpu`, sans bibliothèque de rendu.

*A 3D rendering engine written by hand in Rust, directly on Vulkan — no middleware, no `wgpu`,
no rendering library.*

---

## 🛠️ À propos / About

Le dépôt porte **deux parties séparées**, et la frontière entre elles est tenue par un test :

| | |
|---|---|
| **`crates/aegis_engine`** | le moteur — Vulkan, shaders, lumière, géométrie, mesure |
| **`crates/aegis_game`** | le jeu qui s'en sert, et qui lui sert de banc de validation |

> **Le moteur fournit ce qui est VRAI, le jeu fournit ce qui est BEAU.**
> Un test échoue si un shader du moteur contient une couleur : une couleur est une décision
> d'artiste, elle n'a rien à faire du côté du moteur.

*The engine provides what is TRUE, the game provides what is BEAUTIFUL. A test fails if any engine
shader contains a color.*

### Ce que le moteur demande à votre carte / What the engine requires

**Vulkan 1.3**, et **une seule** fonctionnalité de cette version : `dynamicRendering`.

Rien d'autre n'est exigé, et c'est délibéré — une fonctionnalité demandée sans être utilisée refuse
des machines pour rien. Un test le garantit : `le_moteur_ne_demande_que_ce_qu_il_utilise` échoue si
le moteur réclame quoi que ce soit dont aucun code vivant ne se sert. Et si votre carte ne convient
pas, le moteur vous **dit laquelle des exigences manque**, plutôt que d'échouer sans un mot.

*Vulkan 1.3 and exactly one feature from it: `dynamicRendering`. Nothing else is required, on
purpose — and a test enforces it.*

---

## 🚀 Utilisation / Usage

```bash
./run.sh                # lancer le jeu / run the game
./run.sh --screenshot   # rendre une image et sortir / render one frame and exit
```

Les tests, moteur et jeu :

```bash
nix-shell shell.nix --run "cargo test --release"
```

Le moteur sait aussi ouvrir un contexte Vulkan **sans aucune fenêtre**, ce qui permet à ses tests de
vérifier de vrais pixels produits par la carte graphique — y compris sur une machine sans écran.

*The engine can open a Vulkan context with no window at all, so its tests can check real pixels
produced by the GPU — even on a headless machine.*

---

## 📜 Licence / License

Ce projet est distribué sous la licence [Apache 2.0](LICENSE).

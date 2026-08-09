# Aegis Engine - Principes Directeurs du Projet (Workspace Rules)

Ce document régit les règles de développement locales pour **AegisEngine**.

---

## 1. 🛡️ NO PREMATURE VICTORY — Ne Jamais Crier Victoire Trop Vite

- Ne déclarer « réglé / fixé / prouvé » que lorsque le comportement a été **VÉRIFIÉ en réel par l'utilisateur** sur l'application en cours d'exécution.
- La compilation et les tests unitaires prouvent la défense et la cohérence interne, **jamais le rendu visuel perçu ni l'expérience réelle**.
- Dire « *Hypothèse très probable, à confirmer par ton test en réel* » au lieu de déclarations prématurées.
- Toujours lister ce que la modification **ne prouve pas encore**.

---

## 2. ✨ ÉLÉGANCE ET PERFECTION — Jamais la Force Brute

- Interdiction d'empiler des constantes arbitraires, des rustines de surface ou des coefficients magiques (`* 1.35`) pour simuler un résultat.
- Traiter la cause physique et architecturale réelle du problème (Pipeline Vulkan 1.4, Swapchain, Shaders BSDF).

---

## 3. 🎯 MÉTHODE DE TRAVAIL & DISCIPLINE

- Prose et réflexions en français clair.
- Initiative du doute et critique honnête d'ingénieur.
- Petits pas mesurables : un changement prouvable à la fois.
- Une mesure contradictoire prime sur n'importe quelle hypothèse.

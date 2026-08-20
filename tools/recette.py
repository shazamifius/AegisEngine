#!/usr/bin/env python3
"""Recette : éprouve les fonctions du jeu une par une, par la console de pilotage.

    tools/recette.py                      (jeu local, AEGIS_CONSOLE=1)
    tools/recette.py --hote 192.168.1.72  (une autre machine)

⚠ CE QUE CE SCRIPT REFUSE DE FAIRE : cocher ce qu'il n'a pas vu. Chaque ligne est soit VERIFIEE
(une mesure la soutient), soit NON VUE (la situation ne s'est pas présentée pendant la fenêtre
d'observation) — jamais « probablement bon ». Une recette qui coche par défaut ne sert à rien : elle
transforme l'ignorance en assurance, ce qui est pire que de ne pas tester.
"""
import argparse, socket, sys, time

class Console:
    def __init__(self, hote, port):
        self.hote, self.port = hote, port

    def __call__(self, cmd, timeout=5.0):
        with socket.create_connection((self.hote, self.port), timeout=timeout) as s:
            s.settimeout(timeout)
            f = s.makefile("rw")
            f.readline()
            f.write(cmd + "\n"); f.flush()
            s.settimeout(0.4)
            rep = []
            try:
                while True:
                    l = f.readline()
                    if not l: break
                    rep.append(l.rstrip())
            except socket.timeout:
                pass
            return "\n".join(rep)

    def etat(self):
        """L'état sous forme de dictionnaire clé=valeur."""
        brut = self("etat")
        d, reste = {}, brut
        # `bouchon=AucunSeul { testes: 12 }` contient des espaces : on l'extrait à part avant de
        # découper le reste sur les espaces, sinon toutes les clés suivantes se décalent.
        if "bouchon=" in reste:
            av, ap = reste.split("bouchon=", 1)
            fin = ap.find(" vote=")
            d["bouchon"] = ap[:fin] if fin >= 0 else ap
            reste = av + (ap[fin:] if fin >= 0 else "")
        for m in reste.split():
            if "=" in m:
                k, v = m.split("=", 1)
                d.setdefault(k, v)
        return d

class Recette:
    def __init__(self):
        self.lignes = []
    def verifie(self, nom, ok, preuve):
        self.lignes.append((nom, "VERIFIEE" if ok else "ECHEC", preuve))
    def non_vue(self, nom, pourquoi):
        self.lignes.append((nom, "NON VUE", pourquoi))
    def rapport(self):
        larg = max(len(n) for n, _, _ in self.lignes)
        out = []
        for nom, etat, preuve in self.lignes:
            marque = {"VERIFIEE": "[x]", "ECHEC": "[!]", "NON VUE": "[ ]"}[etat]
            out.append(f"{marque} {nom.ljust(larg)}  {etat:<9} {preuve}")
        v = sum(1 for _, e, _ in self.lignes if e == "VERIFIEE")
        f = sum(1 for _, e, _ in self.lignes if e == "ECHEC")
        n = sum(1 for _, e, _ in self.lignes if e == "NON VUE")
        out.append("")
        out.append(f"{v} verifiees · {f} echecs · {n} non vues (sur {len(self.lignes)})")
        return "\n".join(out)

def main():
    p = argparse.ArgumentParser()
    p.add_argument("--hote", default="127.0.0.1")
    p.add_argument("--port", type=int, default=47820)
    p.add_argument("--duree", type=int, default=200, help="fenetre d'observation, en secondes")
    a = p.parse_args()
    c, r = Console(a.hote, a.port), Recette()

    # ── 1. Le canal lui-même ────────────────────────────────────────────────────────────────
    try:
        e = c.etat()
    except OSError as ex:
        print(f"[recette] {a.hote}:{a.port} injoignable ({ex}).", file=sys.stderr)
        print("          Le jeu tourne-t-il avec AEGIS_CONSOLE=1 ?", file=sys.stderr)
        return 1
    r.verifie("console repond", "phase" in e, f"phase={e.get('phase')}")

    # ── 2. Le pont vers le cœur ─────────────────────────────────────────────────────────────
    env, rec = int(e.get("envoyes", 0)), int(e.get("recus", 0))
    time.sleep(3)
    e2 = c.etat()
    env2, rec2 = int(e2.get("envoyes", 0)), int(e2.get("recus", 0))
    r.verifie("pont : le coeur PARLE", rec2 > rec, f"recus {rec} -> {rec2}")
    if e2.get("phase") == "Running":
        r.verifie("pont : le jeu POUSSE sa pose", env2 > env, f"envoyes {env} -> {env2}")
    else:
        # Le jeu ne diffuse QUE pendant la course : se taire entre les manches est le correctif
        # qui a supprimé les fausses téléportations. Ne pas le compter comme un échec.
        r.non_vue("pont : le jeu POUSSE sa pose", f"hors course (phase={e2.get('phase')})")

    # ── 3. Les phases et leurs minuteurs ────────────────────────────────────────────────────
    vues, pilotage, saut, vote_vu, podium = set(), None, None, None, None
    x_depart = None
    fin = time.time() + a.duree
    interrompue = None
    while time.time() < fin:
        try:
            e = c.etat()
        except OSError:
            interrompue = "le jeu s'est arrete pendant l'observation"
            break
        ph = e.get("phase", "?")
        vues.add(ph)

        if ph in ("Drafting", "Placement"):
            # Entre deux manches le personnage revient au spawn : on repart d'une base propre,
            # sinon le « déplacement » mesuré serait un retour en arrière.
            x_depart = None

        if ph == "Running":
            x = float(e["pos"].strip("()").split(",")[0])
            # ⚠ LA TOUCHE EST RÉAFFIRMÉE À CHAQUE TOUR, et le déplacement retenu est le MAXIMUM.
            #
            # Deux pièges mesurés, qui ont tous deux fait crier « ECHEC » à ce script sur un
            # pilotage éprouvé à +10,35 unités quelques minutes plus tôt :
            #   - en début de course, le personnage reste immobile ~6 secondes : regarder trop tôt
            #     donne +0,00 ;
            #   - il meurt et revient au spawn : un instantané pris après un respawn montre un
            #     déplacement nul, voire négatif.
            # Une recette qui se trompe fait perdre plus de temps qu'elle n'en fait gagner.
            c("touche droite on")
            if x_depart is None:
                x_depart, t_depart = x, time.time()
            else:
                pilotage = max(pilotage if pilotage is not None else 0.0, x - x_depart)
                if saut is None and time.time() - t_depart > 5:
                    c("touche droite off")
                    y_avant = float(e["pos"].strip("()").split(",")[1])
                    c("appui saut")
                    time.sleep(0.4)
                    saut = float(c.etat()["pos"].strip("()").split(",")[1]) - y_avant

        if e.get("vote", "aucun") != "aucun" and vote_vu is None:
            vote_vu = e["vote"]
            c("voter o")  # on éprouve le canal du bulletin
        if ph == "Leaderboard" and podium is None:
            podium = c("joueurs")
        time.sleep(1.0)

    for ph in ("Drafting", "Placement", "Running", "Leaderboard"):
        r.verifie(f"phase {ph}", ph in vues, "vue" if ph in vues else "jamais atteinte")

    if pilotage is None:
        r.non_vue("pilotage : courir", "la course n'a pas duré assez longtemps")
    else:
        r.verifie("pilotage : courir", pilotage > 1.0, f"deplacement x = {pilotage:+.2f}")
    if saut is None:
        r.non_vue("pilotage : sauter", "pas mesuré")
    else:
        r.verifie("pilotage : sauter", abs(saut) > 0.05, f"deplacement y = {saut:+.2f}")

    # ── 4. Le TAS et le vote ────────────────────────────────────────────────────────────────
    try:
        e = c.etat()
    except OSError:
        e = {}
        r.non_vue("TAS : verdict final", "jeu ferme avant la lecture finale")
    if e:
        r.verifie("TAS : rend un verdict", e.get("carte") not in (None, "Inconnue"), f"carte={e.get('carte')}")
    b = e.get("bouchon", "")
    if not e:
        pass
    elif e.get("carte") == "PasTrouvee":
        r.verifie("TAS : designe un bouchon", b.startswith("Bloc"), f"bouchon={b}")
    else:
        r.non_vue("TAS : designe un bouchon", f"carte non bouchee (carte={e.get('carte')})")
    if vote_vu:
        r.verifie("vote : s'ouvre et accepte un bulletin", True, vote_vu)
    else:
        r.non_vue("vote : s'ouvre", "aucune carte bouchee avec bloc designe pendant la fenetre")

    # ── 5. Le multijoueur ───────────────────────────────────────────────────────────────────
    d = int(e.get("distants", 0))
    if d > 0:
        r.verifie("multijoueur : avatars distants", True, f"{d} avatar(s)")
    else:
        r.non_vue("multijoueur : avatars distants", "aucun autre joueur pendant la fenetre")

    if podium:
        r.verifie("leaderboard : liste les joueurs", bool(podium.strip()), podium.replace("\n", " | "))
    else:
        r.non_vue("leaderboard", "phase jamais atteinte")

    if interrompue:
        r.non_vue("observation complete", interrompue)
    print(r.rapport())
    return 0

if __name__ == "__main__":
    sys.exit(main())

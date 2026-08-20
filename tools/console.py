#!/usr/bin/env python3
"""Client de la console de pilotage d'AegisEngine.

Pourquoi il existe : un jeu Vulkan lancé par SSH n'a pas de session graphique. Sans ce canal,
chaque vérification exige un humain devant chaque écran, en même temps — donc rien ne se teste à
deux machines, et surtout rien ne se REJOUE. Un test qu'on ne peut pas rejouer ne prouve rien.

    tools/console.py etat
    tools/console.py --hote 192.168.1.72 etat
    tools/console.py touche droite on / appui saut / voter o / capture /tmp/x.png
    tools/console.py --suivre 30        (un état par seconde, 30 s)

⚠ Le jeu n'ouvre la console que si `AEGIS_CONSOLE=<port>` est dans son environnement : un binaire
lancé normalement n'écoute rien.
"""
import argparse, socket, sys, time

def dialoguer(hote, port, lignes, timeout=5.0):
    with socket.create_connection((hote, port), timeout=timeout) as s:
        s.settimeout(timeout)
        f = s.makefile("rw")
        f.readline()  # bannière
        out = []
        for l in lignes:
            f.write(l + "\n")
            f.flush()
            # `etat` tient sur une ligne ; `joueurs` et `aide` en rendent plusieurs. On lit tant
            # que ça vient, avec un délai court : plus simple qu'un protocole à longueur, et
            # suffisant pour un outil de test local.
            s.settimeout(0.4)
            rep = []
            try:
                while True:
                    ligne = f.readline()
                    if not ligne:
                        break
                    rep.append(ligne.rstrip())
            except socket.timeout:
                pass
            s.settimeout(timeout)
            out.append("\n".join(rep))
        return out

def main():
    p = argparse.ArgumentParser(description="pilote AegisEngine à distance")
    p.add_argument("--hote", default="127.0.0.1")
    p.add_argument("--port", type=int, default=47820)
    p.add_argument("--suivre", type=int, metavar="SECONDES",
                   help="affiche l'état une fois par seconde pendant N secondes")
    p.add_argument("commande", nargs="*", help="ex: etat | touche droite on | appui saut")
    a = p.parse_args()

    try:
        if a.suivre:
            for _ in range(a.suivre):
                print(dialoguer(a.hote, a.port, ["etat"])[0], flush=True)
                time.sleep(1.0)
            return 0
        if not a.commande:
            p.print_help()
            return 2
        for r in dialoguer(a.hote, a.port, [" ".join(a.commande)]):
            print(r)
        return 0
    except OSError as e:
        # Message explicite : « connexion refusée » sur ce port veut presque toujours dire que le
        # jeu tourne SANS `AEGIS_CONSOLE`, pas qu'il est planté.
        print(f"[console] {a.hote}:{a.port} injoignable ({e}).", file=sys.stderr)
        print("          Le jeu tourne-t-il avec AEGIS_CONSOLE=1 ?", file=sys.stderr)
        return 1

if __name__ == "__main__":
    sys.exit(main())

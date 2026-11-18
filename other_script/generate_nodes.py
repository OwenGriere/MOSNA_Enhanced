#!/usr/bin/env python3
"""
Génère des fichiers .parquet contenant des cellules (nodes) placées
aléatoirement sur une surface donnée, un fichier par patient.

Colonnes de chaque fichier :
    cell_id    : identifiant unique de cellule (unique sur l'ensemble des fichiers)
    X          : coordonnée x
    Y          : coordonnée y
    phenotype  : "immune", "cancer" ou "CAF" (tiré aléatoirement)

Nom des fichiers : nodes_patient-<id>.parquet

- --files      : nombre de fichiers (patients) à produire
- --min-nodes / --max-nodes : intervalle du nombre de nodes ; chaque fichier
                  reçoit un nombre tiré aléatoirement dans cet intervalle

Exemple :
    python generate_nodes.py --files 20 --min-nodes 500 --max-nodes 5000 \
        --width 1000 --height 1000 --outdir sortie

Dépendances : pip install pandas pyarrow numpy
"""
import argparse
from pathlib import Path

import numpy as np
import pandas as pd

PHENOTYPES = ["immune", "cancer", "CAF"]


def main():
    p = argparse.ArgumentParser(description="Génère des .parquet de nodes aléatoires.")
    p.add_argument("--files", type=int, default=10, help="Nombre de fichiers (patients) à produire")
    p.add_argument("--min-nodes", type=int, default=100, help="Nombre minimum de nodes par fichier")
    p.add_argument("--max-nodes", type=int, default=1000, help="Nombre maximum de nodes par fichier")
    p.add_argument("--width", type=float, default=1000, help="Largeur de la surface")
    p.add_argument("--height", type=float, default=1000, help="Hauteur de la surface")
    p.add_argument("--outdir", default="nodes_output", help="Dossier de sortie")
    p.add_argument("--first-id", type=int, default=1, help="Id du premier patient")
    p.add_argument("--seed", type=int, default=None, help="Graine aléatoire (reproductibilité)")
    args = p.parse_args()

    if args.files < 1:
        p.error("--files doit être >= 1")
    if args.min_nodes < 0 or args.max_nodes < args.min_nodes:
        p.error("L'intervalle de nodes est invalide (min <= max, min >= 0)")

    rng = np.random.default_rng(args.seed)
    outdir = Path(args.outdir)
    outdir.mkdir(parents=True, exist_ok=True)

    next_cell_id = 0
    for patient_id in range(args.first_id, args.first_id + args.files):
        n = int(rng.integers(args.min_nodes, args.max_nodes + 1))
        df = pd.DataFrame(
            {
                "cell_id": np.arange(next_cell_id, next_cell_id + n, dtype=np.int64),
                "X": rng.uniform(0, args.width, n),
                "Y": rng.uniform(0, args.height, n),
                "phenotype": rng.choice(PHENOTYPES, n),
            }
        )
        next_cell_id += n

        path = outdir / f"nodes_patient-{patient_id}.parquet"
        df.to_parquet(path, index=False)
        print(f"{path.name} : {n} cellules")

    print(f"\n{args.files} fichier(s) généré(s) dans '{outdir}'.")


if __name__ == "__main__":
    main()

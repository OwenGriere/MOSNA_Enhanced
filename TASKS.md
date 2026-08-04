# MOSNA — tâches restantes

État au terme de la session courante. Voir `PROGRESS.log` pour l'historique
détaillé et les divergences assumées, `TESTING.md` pour la discipline TDD.

**Fait** : 966 tests verts, clippy 0 erreur (`-D warnings`), fmt propre,
doctests OK, CI GitHub en place, installation Linux et Windows, manuel bilingue
lisible dans l'interface, et les 24 figures produites et validées sur les vraies
données du dépôt.

Le portage est **fonctionnellement complet** : les trois analyses tournent,
produisent leurs fichiers et leurs figures, l'interface les pilote, et
le dépôt est désormais autonome — plus rien n'est lu hors de sa racine, et
les sources Python d'origine ont été retirées (archive `MOSNA_python-legacy.tar.gz`
à côté du dépôt).

---

## Légende

| Marque | Sens |
|---|---|
| 🔴 | Bloquant : l'application est incomplète sans ça |
| 🟠 | Important : dégrade l'usage ou la confiance dans les résultats |
| 🟡 | Utile : améliore la robustesse ou le confort |
| 🔵 | Proposition de ma part, à arbitrer |

---

## 🔴 Bloquant

### ~~1. `mosna-viz` — les figures~~ ✅ FAIT

Les 24 figures sont produites et validées sur les vraies données. Voir
`PROGRESS.log`, étape 11.

Deux points restent ouverts sur ce sujet, sans être bloquants :

- **Le dendrogramme de `Assortativity_heatmap_with_dendrogram` est
  schématique.** L'ordre des feuilles est le vrai — c'est lui qui rend la
  heatmap lisible — mais les hauteurs de fusion ne sont pas dessinées à
  l'échelle. Les tracer demanderait de faire passer la matrice de linkage
  jusqu'au code de dessin, pour un gain d'interprétation nul.
- **Pas de barre de couleur.** Le Python en dessine une à droite de chaque
  heatmap. L'échelle est donc lisible en forme mais pas en valeur.

### ~~2. Installation Windows~~ ✅ FAIT

`Layout::for_platform` connaît les deux plateformes, `install.ps1` existe, et
le `.lnk` est écrit à la main au format MS-SHLLINK (aucune dépendance COM :
`mslnk` ne compile pas sous Linux, donc il ne pouvait pas valider les tests
écrits d'abord). Les deux installations déposent une icône sur le bureau.

**Un point reste à vérifier à la main** : tout cela est écrit et testé *depuis
Linux*. Il faut lancer `.\install.ps1` sur une vraie machine Windows une fois,
et vérifier que le raccourci démarre bien l'interface et que `eframe` trouve son
backend graphique. La CI compile et teste sur `windows-latest` mais n'exécute
pas l'installation.

---

## 🟠 Important

### 3. Le point ouvert sur les identifiants dans le clustering

**Décision attendue de votre part.** `aggregated_niches` passe
`var_aggreg.values` à UMAP, et ce tableau contient encore les colonnes
`patient` et `sample`. Les identifiants entrent donc dans la réduction comme
des variables numériques ordinaires et influencent les niches obtenues.

C'est reproduit à l'identique dans `VarAggreg::clustering_matrix()` pour que
les labels coïncident avec le Python. Si c'est un oubli côté Python, le retrait
est un changement d'une ligne — mais il fera diverger tous les résultats.

### 4. k plus proches voisins approché (NN-descent)

Le kNN d'UMAP est exact, en `O(n²·d)`. Correct et testé, mais pour une cohorte
de plusieurs centaines de milliers de cellules c'est le goulot d'étranglement.
Le Python bascule sur `pynndescent` au-delà de 3000 points.

À faire : implémenter NN-descent avec un test de rappel (> 0,9 contre la
recherche exhaustive), et un basculement automatique au-delà d'un seuil.
`knn_graph` exact reste la spécification de référence.

### 5. HDBSCAN et ECG

- **ECG** : le Python lève `RuntimeError` sur CPU (cugraph requis). Reproduire
  la même erreur dans `reduce_and_cluster` — actuellement un message générique.
- **HDBSCAN** : proposé par la liste déroulante de la GUI mais **rejeté par
  `assert_params`** (`clusterer_type in ["leiden","ecg","spectral","gmm"]`).
  Incohérence côté Python. Deux options : l'implémenter et l'autoriser, ou le
  retirer de la liste de la GUI. À arbitrer.

### 6. Cache des embeddings

Seul `var_aggreg.parquet` est mis en cache. Le Python cache aussi
`embedding.npy` sous un chemin encodant les paramètres de réduction —
`NicheParams::reducer_name()` est déjà implémenté et testé pour ça, mais rien
ne l'utilise. Relancer une analyse avec un clusterer différent recalcule donc
UMAP inutilement (c'est l'étape la plus coûteuse : 22 s sur 39 k cellules).

À faire : un lecteur/écrivain `.npy` minimal (ou du parquet, mais on perd
l'interopérabilité avec le Python) et le branchement dans `run_aggregated`.

---

## 🟡 Utile

### ~~7. Documentation embarquée~~ ✅ FAIT

Le manuel est un document structuré (`crates/mosna-gui/src/docs/`) que
l'interface dessine elle-même : cinq chapitres, anglais et français, navigation
en arbre, recherche, précédent/suivant, bouton de langue, palette noir et or.
Il inclut le chapitre d'installation que le Python n'avait pas.

Reste ouvert, mineur :

- **Rien ne relie le manuel aux paramètres de l'interface.** Un clic droit sur
  un réglage qui ouvrirait sa ligne de tableau serait le prolongement naturel
  du test qui garantit déjà que chacun est documenté.
- **La recherche est une correspondance de sous-chaîne.** Suffisant pour un
  manuel de cette taille ; une recherche floue serait plus tolérante aux fautes
  de frappe.

### 8. Premier lancement : copier la configuration

`mosna_paths::config_file::user_path()` existe et est testé, mais rien ne copie
la configuration livrée vers `~/.config/mosna/` au premier lancement. Résultat :
un utilisateur qui installe puis sauvegarde écrit dans `share/mosna/`, qui peut
être en lecture seule sur une installation système.

À faire : au démarrage, si la configuration utilisateur n'existe pas et que la
configuration livrée existe, la copier.

### 9. Chemins relatifs dans la configuration

`Nodes directory` est résolu relativement au répertoire de travail. Une
configuration partagée entre machines casse donc dès que les chemins diffèrent.
Le Python a le même comportement — à conserver ou à améliorer sciemment.

### 10. Journalisation

`log` et `env_logger` sont dans les dépendances du workspace mais ne sont
utilisés nulle part. Soit on branche une vraie journalisation (utile pour
diagnostiquer un run long), soit on retire les dépendances.

---

## 🔵 Propositions

Choses que je n'ai pas faites parce que vous ne les avez pas demandées, mais
qui me semblent valoir la peine.

### 11. Un test de non-régression numérique bout en bout

Aujourd'hui les tests vérifient des invariants ; rien ne détecterait une
dérive numérique lente entre deux versions. Un test qui fait tourner les trois
étapes sur `test/patient_sample_folder` et compare `net_stat.csv` à une
référence versionnée (avec tolérance) attraperait ça.

Coût : quelques minutes de CI. Bénéfice : la garantie que refactorer le cœur ne
change pas les résultats.

### 12. Comparaison automatisée Python ↔ Rust

Le vrai juge de paix du portage. Un script qui fait tourner les deux
implémentations sur le même jeu de données et compare les sorties, avec les
divergences connues (permutations aléatoires, UMAP non reproductible côté
Python) listées comme attendues.

C'est ce qui permettrait d'affirmer « les résultats sont les mêmes » plutôt que
« les invariants sont respectés ». Demande un environnement Python fonctionnel
(actuellement `pyarrow` manque dans l'environnement de test).

### ~~13. Mesures de performance versionnées~~ ✅ FAIT

`benchmark/` : trois niveaux (dérive numérique, reproductibilité, récupération
des niches plantées) plus un sweep de temps et de mémoire. Voir
`benchmark/README.md` et `PROGRESS.log` étape 13.

Il a trouvé un vrai défaut dès sa première exécution : Leiden départageait les
gains égaux par l'ordre d'itération d'une `HashMap`, graine par thread. Corrigé.

### 14. Barre de progression pour l'étape 3

L'étape 3 est la seule longue (22 s sur 39 k cellules, bien plus sur une vraie
cohorte) et n'émet que trois pas de progression. UMAP et le GMM pourraient
rapporter leur avancement par époque et par itération EM — le protocole
`[QT_PROGRESS]` le permet déjà.

### 15. Reprise après interruption

Un run interrompu à l'étape 3 laisse `var_aggreg.parquet` mais pas les niches.
C'est déjà repris au relancement. En revanche l'étape 1 recalcule tout, même si
les fichiers réseau existent. Un saut des échantillons déjà traités ferait
gagner beaucoup sur les grosses cohortes.

### 16. Empaquetage

Au-delà du script d'installation : un `.deb`, un AppImage ou un Flatpak
rendraient la distribution beaucoup plus simple pour des utilisateurs non
techniques. AppImage me semble le meilleur rapport effort/bénéfice ici (un seul
fichier, aucune dépendance système).

### 17. Message d'erreur quand aucun échantillon n'est trouvé

`PipelineError::NoSamples` donne le dossier et le motif, mais pas ce qui a été
trouvé à la place. Lister les deux ou trois premiers fichiers présents
transformerait « rien trouvé » en « vous avez écrit `patient` mais les fichiers
disent `Patient` ».

### 18. Retirer `mosna-testkit` de la compilation de release

C'est une dépendance de développement, mais elle est membre du workspace donc
`cargo build --workspace` la compile. Sans conséquence, juste du temps perdu.

### 19. Vérifier l'installation Windows sur une vraie machine

Le seul point de tout le projet qui n'a jamais tourné sur sa plateforme cible —
et il en couvre trois désormais : `bootstrap.ps1`, `install.ps1` et le `.lnk`
écrit à la main. Aucun PowerShell n'existe sur la machine de développement.

Une session de vingt minutes sous Windows suffirait : coller la ligne unique du
README, double-cliquer sur le raccourci du bureau, lancer une analyse, puis
`.\install.ps1 -Uninstall`.

### 20. Ajouter une licence

Le dépôt n'en contient plus depuis la suppression du paquet Python. Sans
fichier de licence, le code est par défaut « tous droits réservés », ce qui
empêche vos collaborateurs de le réutiliser légalement.

### 21. Réduction déterministe du null par permutation

Les z-scores diffèrent au quinzième chiffre entre deux nombres de threads :
`randomized_mixmat` utilise `rayon::reduce`, dont l'arbre de fusion dépend du
découpage, et l'addition flottante n'est pas associative.

Rendre cela exact : replier sur un nombre **fixe** de fragments (indépendant du
nombre de threads), puis fusionner ces fragments dans l'ordre de leur indice.
Coût : garder K accumulateurs en mémoire, soit K x n² x 16 octets — 1,2 Mo pour
34 phénotypes et K=64, mais 92 Mo pour 300 phénotypes.

Mon avis : pas rentable pour 1e-15, sauf si vous publiez des z-scores qui
doivent être reproductibles au bit près.

### ~~22. Faire tourner le niveau 1 du banc en CI~~ ✅ FAIT

Job « numerical drift » dans `.github/workflows/rust.yml` : niveau 1 (16 ms) et
niveau 2 (1,4 s), tous deux en release.

### 23. Regarder l'interface agrandie et arbitrer l'échelle

Les tailles sont vérifiées par `crates/mosna-gui/tests/theme_scale.rs` mais
**jamais vues** : la session est en Wayland et le compositeur refuse les
captures. Si quelque chose est trop gros ou trop petit, tout part du module
`size` de `crates/mosna-gui/src/theme.rs` — un chiffre changé s'y propage
partout.

### 24. Publier des binaires Windows prêts à l'emploi

Aujourd'hui installer sous Windows demande d'installer Rust puis de compiler.
Un job GitHub Actions sur `windows-latest` qui construit `mosna.exe` et
`mosna-gui.exe` et les attache à une Release supprimerait les deux étapes : le
lien du README téléchargerait l'application, pas de quoi la fabriquer.

Point à connaître : un `.exe` non signé déclenche l'avertissement SmartScreen.
Un certificat de signature de code coûte quelques centaines d'euros par an ;
sans lui, il faut documenter le « Informations complémentaires → Exécuter quand
même ».

### 25. Une désinstallation avec un préfixe vide ne devrait pas retirer les raccourcis

`install.sh --uninstall --prefix /un/chemin/sans/rien` supprime quand même le
lanceur du bureau et l'entrée du menu Démarrer : les raccourcis sont calculés
depuis l'environnement, pas depuis le préfixe. C'est ainsi qu'un test a effacé
un vrai lanceur pendant cette session.

Correction : ne retirer les raccourcis que s'ils pointent vers un binaire du
préfixe qu'on désinstalle. Le `.desktop` porte déjà son `Exec=`, et le `.lnk`
son chemin cible — les deux sont lisibles avant suppression.

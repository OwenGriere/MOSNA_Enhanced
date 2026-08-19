//! The manual's text, in English and French.
//!
//! Adapted from the Python interface's HTML manual, with an installation chapter the
//! Python version does not have — the Python was always run from a checkout,
//! this is installed.
//!
//! Keeping the text here rather than in a data file means the compiler checks
//! its structure, and the test suite can compare it against what the interface
//! actually offers.

use super::model::{
    Block, CalloutKind, Chapter, Citation as C, ParameterRow as Row, Section, Text as T,
};

/// Assemble every chapter.
pub fn chapters() -> Vec<Chapter> {
    vec![
        installation(),
        getting_started(),
        workflow(),
        parameters(),
        results(),
        credits(),
    ]
}

// ---------------------------------------------------------------------------
// Installation
// ---------------------------------------------------------------------------

fn installation() -> Chapter {
    Chapter {
        id: "installation",
        title: T::new("Installation", "Installation"),
        sections: vec![
            Section {
                id: "install-requirements",
                title: T::new("Requirements", "Prérequis"),
                blocks: vec![
                    Block::Paragraph(T::new(
                        "MOSNA is a native application: everything it computes, it computes \
                         itself — no conda environment, no scientific stack.",
                        "MOSNA est une application native : tout ce qu'elle calcule, elle le \
                         calcule elle-même — pas d'environnement conda, pas de pile \
                         scientifique.",
                    )),
                    Block::Paragraph(T::new(
                        "The figures are the one exception. They are drawn by xy, a Python \
                         charting library, which is what makes each of them both an image and \
                         a chart you can pan, zoom and read values off. So the machine needs \
                         Python 3.11 or newer — and nothing else from it: the installer builds \
                         a small environment of its own under the install directory and puts \
                         the renderer there, leaving whatever Python you work in untouched.",
                        "Les figures sont la seule exception. Elles sont dessinées par xy, une \
                         bibliothèque graphique Python, ce qui fait de chacune à la fois une \
                         image et un graphique que l'on peut déplacer, zoomer et dont on peut \
                         lire les valeurs. La machine a donc besoin de Python 3.11 ou plus \
                         récent — et de rien d'autre : l'installateur construit un petit \
                         environnement à lui sous le dossier d'installation et y place le \
                         moteur de rendu, sans toucher au Python dans lequel vous travaillez.",
                    )),
                    Block::Paragraph(T::new(
                        "Building it requires the Rust toolchain. Installing it requires \
                         nothing beyond your own user account — no administrator rights.",
                        "La construire demande la chaîne d'outils Rust. L'installer ne demande \
                         rien de plus que votre compte utilisateur — aucun droit administrateur.",
                    )),
                    Block::Code {
                        caption: T::new(
                            "Install the Rust toolchain, once",
                            "Installer la chaîne d'outils Rust, une seule fois",
                        ),
                        lines: vec![
                            "curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh",
                        ],
                    },
                    Block::Code {
                        caption: T::new(
                            "And Python, if the machine has none",
                            "Et Python, si la machine n'en a pas",
                        ),
                        lines: vec![
                            "sudo apt install python3 python3-venv    # Debian, Ubuntu, Mint",
                            "sudo dnf install python3                 # Fedora, RHEL",
                            "sudo pacman -S python                    # Arch, Manjaro",
                            "winget install Python.Python.3.13        # Windows",
                        ],
                    },
                    Block::Callout {
                        kind: CalloutKind::Note,
                        text: T::new(
                            "Setting MOSNA_PYTHON points MOSNA at a particular interpreter, \
                             which is what to do when you already keep an environment with xy \
                             in it.",
                            "Définir MOSNA_PYTHON désigne un interpréteur particulier, ce qu'il \
                             faut faire si vous entretenez déjà un environnement contenant xy.",
                        ),
                    },
                    Block::Callout {
                        kind: CalloutKind::Note,
                        text: T::new(
                            "Open a new terminal after installing Rust, so that the new \
                             commands are on your PATH.",
                            "Ouvrez un nouveau terminal après avoir installé Rust, pour que \
                             les nouvelles commandes soient dans votre PATH.",
                        ),
                    },
                ],
            },
            Section {
                id: "install-linux",
                title: T::new("Installing on Linux", "Installer sous Linux"),
                blocks: vec![
                    Block::Paragraph(T::new(
                        "From the MOSNA_Rust directory, one command builds the application \
                         and installs it into your home. The first build takes a few minutes.",
                        "Depuis le dossier MOSNA_Rust, une seule commande construit \
                         l'application et l'installe dans votre dossier personnel. La première \
                         construction prend quelques minutes.",
                    )),
                    Block::Code {
                        caption: T::new("Install", "Installer"),
                        lines: vec!["cd MOSNA_Rust", "./install.sh"],
                    },
                    Block::Paragraph(T::new(
                        "This places the two programs in ~/.local/bin, adds MOSNA to your \
                         application menu, and puts a launcher on your desktop — a double \
                         click starts the interface.",
                        "Cela place les deux programmes dans ~/.local/bin, ajoute MOSNA à votre \
                         menu d'applications, et dépose un lanceur sur votre bureau — un double \
                         clic démarre l'interface.",
                    )),
                    Block::List(vec![
                        T::new(
                            "mosna-gui — the graphical interface",
                            "mosna-gui — l'interface graphique",
                        ),
                        T::new(
                            "mosna — the command line, for scripts and clusters",
                            "mosna — la ligne de commande, pour les scripts et les calculateurs",
                        ),
                    ]),
                    Block::Code {
                        caption: T::new("Other options", "Autres options"),
                        lines: vec![
                            "./install.sh --dry-run              # show what would happen",
                            "./install.sh --prefix /usr/local    # install for everyone",
                            "./install.sh --uninstall            # remove it again",
                        ],
                    },
                    Block::Callout {
                        kind: CalloutKind::Tip,
                        text: T::new(
                            "If your desktop asks whether to trust the launcher, allow it \
                             once. The installer already marks the file executable, which is \
                             what most desktops need.",
                            "Si votre bureau demande s'il faut faire confiance au lanceur, \
                             autorisez-le une fois. L'installeur marque déjà le fichier comme \
                             exécutable, ce qu'attendent la plupart des bureaux.",
                        ),
                    },
                ],
            },
            Section {
                id: "install-windows",
                title: T::new("Installing on Windows", "Installer sous Windows"),
                blocks: vec![
                    Block::Paragraph(T::new(
                        "Install the Rust toolchain from rustup.rs, then run the installer \
                         from PowerShell in the MOSNA_Rust directory.",
                        "Installez la chaîne d'outils Rust depuis rustup.rs, puis lancez \
                         l'installeur depuis PowerShell, dans le dossier MOSNA_Rust.",
                    )),
                    Block::Code {
                        caption: T::new("Install", "Installer"),
                        lines: vec!["cd MOSNA_Rust", ".\\install.ps1"],
                    },
                    Block::Paragraph(T::new(
                        "This installs into %LOCALAPPDATA%\\Programs\\MOSNA and creates two \
                         shortcuts: one in the Start Menu and one on your desktop.",
                        "Cela installe dans %LOCALAPPDATA%\\Programs\\MOSNA et crée deux \
                         raccourcis : un dans le menu Démarrer et un sur votre bureau.",
                    )),
                    Block::Code {
                        caption: T::new("Other options", "Autres options"),
                        lines: vec![
                            ".\\install.ps1 -DryRun       # show what would happen",
                            ".\\install.ps1 -Uninstall    # remove it again",
                        ],
                    },
                    Block::Callout {
                        kind: CalloutKind::Warning,
                        text: T::new(
                            "PowerShell may refuse to run a downloaded script. If it does, \
                             allow it for this session only with: \
                             Set-ExecutionPolicy -Scope Process -ExecutionPolicy Bypass",
                            "PowerShell peut refuser d'exécuter un script téléchargé. Le cas \
                             échéant, autorisez-le pour cette session seulement avec : \
                             Set-ExecutionPolicy -Scope Process -ExecutionPolicy Bypass",
                        ),
                    },
                ],
            },
            Section {
                id: "install-after",
                title: T::new("After installing", "Après l'installation"),
                blocks: vec![
                    Block::Paragraph(T::new(
                        "Start MOSNA from the desktop icon, from your application menu, or \
                         from a terminal.",
                        "Démarrez MOSNA depuis l'icône du bureau, depuis votre menu \
                         d'applications, ou depuis un terminal.",
                    )),
                    Block::Code {
                        caption: T::new("From a terminal", "Depuis un terminal"),
                        lines: vec!["mosna-gui", "mosna --help"],
                    },
                    Block::Paragraph(T::new(
                        "Your settings are kept separately from the installed copy, so \
                         reinstalling or upgrading never overwrites them.",
                        "Vos réglages sont conservés séparément de la copie installée : \
                         réinstaller ou mettre à jour ne les écrase jamais.",
                    )),
                    Block::Callout {
                        kind: CalloutKind::Note,
                        text: T::new(
                            "If the commands are not found, ~/.local/bin is not on your PATH. \
                             The installer says so at the end and prints the full path you can \
                             use instead.",
                            "Si les commandes sont introuvables, ~/.local/bin n'est pas dans \
                             votre PATH. L'installeur le signale à la fin et affiche le chemin \
                             complet que vous pouvez utiliser à la place.",
                        ),
                    },
                ],
            },
        ],
    }
}

// ---------------------------------------------------------------------------
// Getting started
// ---------------------------------------------------------------------------

fn getting_started() -> Chapter {
    Chapter {
        id: "getting-started",
        title: T::new("Getting started", "Premiers pas"),
        sections: vec![
            Section {
                id: "purpose",
                title: T::new("What this interface is for", "À quoi sert cette interface"),
                blocks: vec![
                    Block::Paragraph(T::new(
                        "This interface analyses spatial omics data using networks. It drives \
                         Tysserand and MOSNA step by step, so you can run a whole spatial \
                         analysis without writing code.",
                        "Cette interface analyse des données omiques spatiales à l'aide de \
                         réseaux. Elle pilote Tysserand et MOSNA étape par étape, ce qui permet \
                         de mener une analyse spatiale complète sans écrire de code.",
                    )),
                    Block::List(vec![
                        T::new(
                            "Step 1 — Tysserand reconstructs a spatial network per sample.",
                            "Étape 1 — Tysserand reconstruit un réseau spatial par échantillon.",
                        ),
                        T::new(
                            "Step 2 — Assortativity measures which cell types sit next to which.",
                            "Étape 2 — L'assortativité mesure quels types cellulaires voisinent \
                             avec quels autres.",
                        ),
                        T::new(
                            "Step 3 — Niche Analysis groups neighbourhoods into spatial niches.",
                            "Étape 3 — L'analyse de niches regroupe les voisinages en niches \
                             spatiales.",
                        ),
                    ]),
                ],
            },
            Section {
                id: "working-directory",
                title: T::new(
                    "Start-up and working directory",
                    "Démarrage et répertoire de travail",
                ),
                blocks: vec![
                    Block::Paragraph(T::new(
                        "On launch you must choose a folder. Without a working directory the \
                         interface stays disabled.",
                        "Au démarrage, vous devez choisir un dossier. Sans répertoire de \
                         travail, l'interface reste désactivée.",
                    )),
                    Block::Paragraph(T::new(
                        "That folder is where everything is written: the results, and the \
                         intermediate network files under temp.",
                        "C'est dans ce dossier que tout est écrit : les résultats, et les \
                         fichiers réseau intermédiaires sous temp.",
                    )),
                    Block::Callout {
                        kind: CalloutKind::Tip,
                        text: T::new(
                            "Create a new folder at the moment you choose the working \
                             directory. That folder becomes your analysis.",
                            "Créez un nouveau dossier au moment de choisir le répertoire de \
                             travail. Ce dossier devient votre analyse.",
                        ),
                    },
                ],
            },
            Section {
                id: "input-files",
                title: T::new("Input files", "Fichiers d'entrée"),
                blocks: vec![
                    Block::Paragraph(T::new(
                        "You need at least one CSV or Parquet file listing cells with their \
                         coordinates and attributes. Tysserand turns those into nodes and \
                         edges.",
                        "Il vous faut au moins un fichier CSV ou Parquet listant les cellules \
                         avec leurs coordonnées et leurs attributs. Tysserand en tire des \
                         noeuds et des arêtes.",
                    )),
                    Block::Paragraph(T::new(
                        "If you have no coordinates but already have edges, you can skip \
                         straight to assortativity and niche analysis, provided the files \
                         follow the naming below.",
                        "Si vous n'avez pas de coordonnées mais déjà des arêtes, vous pouvez \
                         passer directement à l'assortativité et à l'analyse de niches, à \
                         condition que les fichiers respectent le nommage ci-dessous.",
                    )),
                    Block::Code {
                        caption: T::new("File naming", "Nommage des fichiers"),
                        lines: vec![
                            "# For Tysserand (step 1):",
                            "nodes_{patient-name}-{patient-id}_{sample-name}-{sample-id}.parquet",
                            "",
                            "# To start directly at step 2 or 3, two files per sample:",
                            "nodes_{patient-name}-{patient-id}_{sample-name}-{sample-id}.parquet",
                            "edges_{patient-name}-{patient-id}_{sample-name}-{sample-id}.parquet",
                        ],
                    },
                    Block::Paragraph(T::new(
                        "The second level is optional: for a dataset with patients only, \
                         leave the sample column name empty and name the files \
                         nodes_patient-01.parquet.",
                        "Le second niveau est facultatif : pour un jeu de données par patient \
                         seulement, laissez le nom de colonne d'échantillon vide et nommez les \
                         fichiers nodes_patient-01.parquet.",
                    )),
                    Block::Callout {
                        kind: CalloutKind::Warning,
                        text: T::new(
                            "If the patient or sample column name does not match what the file \
                             names actually contain, the table stays empty. That is the most \
                             common reason nothing appears.",
                            "Si le nom de colonne patient ou échantillon ne correspond pas à ce \
                             que contiennent réellement les noms de fichiers, la table reste \
                             vide. C'est la raison la plus fréquente pour laquelle rien \
                             n'apparaît.",
                        ),
                    },
                    Block::Paragraph(T::new(
                        "Press 'Refresh Nodes' to list your raw files, and 'Refresh Networks' \
                         to list reconstructed networks.",
                        "Appuyez sur « Refresh Nodes » pour lister vos fichiers bruts, et sur \
                         « Refresh Networks » pour lister les réseaux reconstruits.",
                    )),
                ],
            },
        ],
    }
}

// ---------------------------------------------------------------------------
// How it works
// ---------------------------------------------------------------------------

fn workflow() -> Chapter {
    Chapter {
        id: "workflow",
        title: T::new("How it works", "Fonctionnement"),
        sections: vec![
            Section {
                id: "panels",
                title: T::new("The three panels", "Les trois panneaux"),
                blocks: vec![
                    Block::List(vec![
                        T::new(
                            "Browser, on the left — where your data is and how the files are \
                             named. Nothing is computed here; it prepares the paths.",
                            "Browser, à gauche — où sont vos données et comment les fichiers \
                             sont nommés. Rien n'y est calculé ; il prépare les chemins.",
                        ),
                        T::new(
                            "Viewer, in the middle — figures, the network drawn from the files \
                             themselves, the log of a running analysis, and this manual.",
                            "Viewer, au centre — les figures, le réseau tracé directement \
                             depuis les fichiers, le journal de l'analyse en cours, et ce \
                             manuel.",
                        ),
                        T::new(
                            "Parameters, on the right — every setting, plus the buttons that \
                             run the three steps.",
                            "Parameters, à droite — tous les réglages, ainsi que les boutons qui \
                             lancent les trois étapes.",
                        ),
                    ]),
                    Block::Paragraph(T::new(
                        "Selecting a file in the Browser reads its column names, so the \
                         parameter drop-downs offer real columns instead of free text.",
                        "Sélectionner un fichier dans le Browser lit ses noms de colonnes : les \
                         listes déroulantes des paramètres proposent alors de vraies colonnes \
                         plutôt qu'un texte libre.",
                    )),
                    Block::Image {
                        asset: "images/GUI.png",
                        caption: T::new(
                            "The three panels of the interface",
                            "Les trois panneaux de l'interface",
                        ),
                    },
                    Block::Paragraph(T::new(
                        "The Viewer's Network tab draws a sample from its nodes and edges \
                         files rather than from a figure. Choose the patient, then the sample; \
                         a dataset with no sample column asks only for the patient. Drag to \
                         pan, scroll or use the zoom buttons, and hover a cell to read the \
                         columns you ticked in the margin. Colouring by a column of labels — a \
                         phenotype, a niche — gives a legend of its values; colouring by a \
                         measured column gives a colour bar over its range. Past sixty \
                         thousand cells in view only a fraction are drawn, with the edges \
                         between them: at that size a cell is under a pixel and they overlap \
                         several deep, so the rest would cost frames and show nothing. Zoom \
                         in and they all come back.",
                        "L'onglet Network du Viewer trace un échantillon depuis ses fichiers de \
                         nœuds et d'arêtes plutôt que depuis une figure. Choisissez le patient, \
                         puis l'échantillon ; un jeu de données sans colonne d'échantillon ne \
                         demande que le patient. Glisser pour se déplacer, molette ou boutons \
                         de zoom, survoler une cellule pour lire les colonnes cochées en \
                         marge. Colorer par une colonne d'étiquettes — un phénotype, une niche \
                         — donne une légende de ses valeurs ; colorer par une colonne mesurée \
                         donne une barre de couleur sur son étendue. Au-delà de soixante mille \
                         cellules à l'écran, seule une fraction est tracée, avec les arêtes qui \
                         les relient : à cette taille une cellule fait moins d'un pixel et \
                         elles se recouvrent, le reste coûterait des images par seconde sans \
                         rien montrer. En zoomant, tout revient.",
                    )),
                ],
            },
            Section {
                id: "architecture",
                title: T::new("How the analysis is organised", "Organisation de l'analyse"),
                blocks: vec![
                    Block::Paragraph(T::new(
                        "Your working directory is also your saving directory. Each step \
                         writes into it, and the next step reads what the previous one wrote.",
                        "Votre répertoire de travail est aussi votre répertoire de sauvegarde. \
                         Chaque étape y écrit, et l'étape suivante lit ce que la précédente a \
                         écrit.",
                    )),
                    Block::Code {
                        caption: T::new(
                            "What lands in the working directory",
                            "Ce qui atterrit dans le répertoire de travail",
                        ),
                        lines: vec![
                            "temp/net_dir_mosna/   nodes_*.parquet, edges_*.parquet",
                            "Tysserand_Network/    net_{patient}-{sample}.png and .html",
                            "Assortativity/        net_stat.csv and its figures",
                            "Niche_Analysis/       niches, their composition, the projection",
                            "report.html           every figure above, on one page",
                        ],
                    },
                    Block::Paragraph(T::new(
                        "Under the three step buttons sit two more, side by side. Neither \
                         computes anything: they are what you do with a directory the steps \
                         have already filled.",
                        "Sous les trois boutons d'étape s'en trouvent deux autres, côte à \
                         côte. Aucun des deux ne calcule quoi que ce soit : ils servent à \
                         traiter un répertoire que les étapes ont déjà rempli.",
                    )),
                    Block::List(vec![
                        T::new(
                            "Generate report — walks the working directory and writes \
                             report.html next to the results. One tab per analysis; inside \
                             each, the figures about the whole cohort first and then one \
                             patient at a time; and a box at the top that filters everything \
                             by patient, by sample or by file name. Figures are shown small, \
                             several to a row; clicking one opens it large, and the chart \
                             inside it zooms and pans as it does anywhere else. Press it as \
                             often as you like: it replaces the previous report rather than \
                             adding to it.",
                            "Generate report — parcourt le répertoire de travail et écrit \
                             report.html à côté des résultats. Un onglet par analyse ; dans \
                             chacun, d'abord les figures qui portent sur toute la cohorte, \
                             puis un patient à la fois ; et en haut un champ qui filtre le \
                             tout par patient, par échantillon ou par nom de fichier. Les \
                             figures sont affichées en petit, plusieurs par ligne ; un clic \
                             en ouvre une en grand, et le graphique qu'elle contient se zoome \
                             et se déplace comme partout ailleurs. Appuyez autant de fois que \
                             voulu : il remplace le rapport précédent au lieu de s'y ajouter.",
                        ),
                        T::new(
                            "Clear temporary data — deletes the temp folder and the \
                             intermediate networks in it. The figures, the tables and the \
                             report are kept.",
                            "Clear temporary data — supprime le dossier temp et les réseaux \
                             intermédiaires qu'il contient. Les figures, les tableaux et le \
                             rapport sont conservés.",
                        ),
                    ]),
                    Block::Callout {
                        kind: CalloutKind::Tip,
                        text: T::new(
                            "Once step 1 has run you can clear the temporary files at any \
                             time, but steps 2 and 3 read them — clear them only when you are \
                             done. The report reads only the figures, so it can be generated \
                             before or after clearing, and either way describes what is left.",
                            "Une fois l'étape 1 passée, vous pouvez vider les fichiers \
                             temporaires à tout moment, mais les étapes 2 et 3 les lisent — ne \
                             les videz qu'une fois terminé. Le rapport ne lit que les figures : \
                             il peut être généré avant ou après le vidage, et décrit dans les \
                             deux cas ce qui reste.",
                        ),
                    },
                    Block::Image {
                        asset: "images/workflow.png",
                        caption: T::new("The analysis workflow", "Le déroulé de l'analyse"),
                    },
                ],
            },
        ],
    }
}

// ---------------------------------------------------------------------------
// Parameters
// ---------------------------------------------------------------------------

fn parameters() -> Chapter {
    let headers = [
        T::new("Parameter", "Paramètre"),
        T::new("Type", "Type"),
        T::new("Description", "Description"),
    ];

    Chapter {
        id: "parameters",
        title: T::new("Parameters", "Paramètres"),
        sections: vec![
            Section {
                id: "parameters-global",
                title: T::new("Shared parameters", "Paramètres communs"),
                blocks: vec![
                    Block::Paragraph(T::new(
                        "These five live in the Browser panel and are shared by all three \
                         steps, so the steps always agree on how your files are named.",
                        "Ces cinq paramètres se trouvent dans le panneau Browser et sont \
                         partagés par les trois étapes, afin qu'elles s'accordent toujours sur \
                         le nommage de vos fichiers.",
                    )),
                    Block::Table {
                        headers,
                        rows: vec![
                            Row::new(
                                "Nodes directory",
                                "string",
                                T::new(
                                    "Folder holding your spatial data.",
                                    "Dossier contenant vos données spatiales.",
                                ),
                            ),
                            Row::new(
                                "Network directory",
                                "string",
                                T::new(
                                    "Folder holding your nodes and edges. Leave on Default to use \
                                 what step 1 produced.",
                                    "Dossier contenant vos noeuds et arêtes. Laissez sur Default \
                                 pour utiliser ce qu'a produit l'étape 1.",
                                ),
                            ),
                            Row::new(
                                "Patient column name",
                                "string",
                                T::new(
                                    "Name of the first level of division, for example 'patient'.",
                                    "Nom du premier niveau de découpage, par exemple « patient ».",
                                ),
                            ),
                            Row::new(
                                "Sample column name",
                                "string",
                                T::new(
                                    "Name of the second level, if there is one. Leave empty \
                                 otherwise.",
                                    "Nom du second niveau, s'il existe. Laissez vide sinon.",
                                ),
                            ),
                            Row::new(
                                "Extension",
                                "string",
                                T::new(
                                    "File format of your input: csv, tsv or parquet.",
                                    "Format de vos fichiers d'entrée : csv, tsv ou parquet.",
                                ),
                            ),
                        ],
                    },
                ],
            },
            Section {
                id: "parameters-tysserand",
                title: T::new("Step 1 — Tysserand", "Étape 1 — Tysserand"),
                blocks: vec![Block::Table {
                    headers,
                    rows: vec![
                        Row::new(
                            "X coordinates column",
                            "string",
                            T::new(
                                "Column holding the X spatial coordinate.",
                                "Colonne contenant la coordonnée spatiale X.",
                            ),
                        ),
                        Row::new(
                            "Y coordinates column",
                            "string",
                            T::new(
                                "Column holding the Y spatial coordinate.",
                                "Colonne contenant la coordonnée spatiale Y.",
                            ),
                        ),
                        Row::new(
                            "Phenotype column",
                            "string",
                            T::new(
                                "Column giving the phenotype of each cell.",
                                "Colonne donnant le phénotype de chaque cellule.",
                            ),
                        ),
                        Row::new(
                            "Edges method",
                            "string",
                            T::new(
                                "How edges are drawn: delaunay triangulates, knn joins each cell \
                             to its nearest neighbours.",
                                "Comment les arêtes sont tracées : delaunay triangule, knn relie \
                             chaque cellule à ses plus proches voisines.",
                            ),
                        ),
                        Row::new(
                            "Min neighbors",
                            "int",
                            T::new(
                                "Minimum number of neighbours each cell must keep. Cells left \
                             below it are reconnected.",
                                "Nombre minimal de voisins que chaque cellule doit conserver. Les \
                             cellules en dessous sont reconnectées.",
                            ),
                        ),
                        Row::new(
                            "CPU",
                            "int",
                            T::new(
                                "How many cores to use. Capped by the machine and by the number \
                             of samples.",
                                "Nombre de coeurs à utiliser. Plafonné par la machine et par le \
                             nombre d'échantillons.",
                            ),
                        ),
                    ],
                }],
            },
            Section {
                id: "parameters-assortativity",
                title: T::new("Step 2 — Assortativity", "Étape 2 — Assortativité"),
                blocks: vec![Block::Table {
                    headers,
                    rows: vec![
                        Row::new(
                            "Phenotype column",
                            "string",
                            T::new(
                                "Column giving the phenotype of each cell.",
                                "Colonne donnant le phénotype de chaque cellule.",
                            ),
                        ),
                        Row::new(
                            "Index",
                            "string",
                            T::new(
                                "Column identifying each cell. Leave on 'index' to use row order.",
                                "Colonne identifiant chaque cellule. Laissez sur « index » pour \
                             utiliser l'ordre des lignes.",
                            ),
                        ),
                        Row::new(
                            "Number of shuffle",
                            "int",
                            T::new(
                                "How many randomisations build the null distribution. More is \
                             more precise and slower.",
                                "Nombre de permutations construisant la distribution nulle. \
                             Davantage est plus précis et plus lent.",
                            ),
                        ),
                        Row::new(
                            "Randomization diagnostic",
                            "bool",
                            T::new(
                                "Run a short timing probe instead of the full analysis, to \
                             estimate how long the real run will take.",
                                "Lance une brève mesure de temps au lieu de l'analyse complète, \
                             pour estimer la durée du vrai calcul.",
                            ),
                        ),
                    ],
                }],
            },
            Section {
                id: "parameters-niches",
                title: T::new("Step 3 — Niche Analysis", "Étape 3 — Analyse de niches"),
                blocks: vec![
                    Block::Table {
                        headers,
                        rows: vec![
                            Row::new("Saving directory", "string", T::new(
                                "Name of the sub-folder for this run, so several analyses can \
                                 live side by side.",
                                "Nom du sous-dossier de ce calcul, pour que plusieurs analyses \
                                 coexistent.")),
                            Row::new("Phenotype column", "string", T::new(
                                "Column giving the phenotype of each cell, used to describe \
                                 what each niche is made of.",
                                "Colonne donnant le phénotype de chaque cellule, utilisée pour \
                                 décrire la composition de chaque niche.")),
                            Row::new("Column to aggregate", "string or list", T::new(
                                "Column or columns aggregated over each neighbourhood. One \
                                 categorical column is one-hot encoded; several numeric \
                                 columns are used as they are.",
                                "Colonne ou colonnes agrégées sur chaque voisinage. Une seule \
                                 colonne catégorielle est encodée en indicatrices ; plusieurs \
                                 colonnes numériques sont utilisées telles quelles.")),
                            Row::new("Processing method", "string", T::new(
                                "Whether niches are called once over the pooled cohort, or \
                                 independently per sample.",
                                "Si les niches sont déterminées une fois sur la cohorte \
                                 entière, ou indépendamment par échantillon.")),
                            Row::new("Niches method", "string", T::new(
                                "How neighbourhood features are computed. NAS aggregates the \
                                 attributes of each cell's neighbours.",
                                "Comment les caractéristiques de voisinage sont calculées. NAS \
                                 agrège les attributs des voisins de chaque cellule.")),
                            Row::new("Plot Network", "bool", T::new(
                                "Redraw each network coloured by niche once the niches are \
                                 found.",
                                "Redessine chaque réseau coloré par niche une fois les niches \
                                 trouvées.")),
                            Row::new("X coordinates column for niches", "string", T::new(
                                "X column used for that redraw.",
                                "Colonne X utilisée pour ce redessin.")),
                            Row::new("Y coordinates column for niches", "string", T::new(
                                "Y column used for that redraw.",
                                "Colonne Y utilisée pour ce redessin.")),
                            Row::new("CPU", "int", T::new(
                                "How many cores to use.",
                                "Nombre de coeurs à utiliser.")),
                        ],
                    },
                    Block::Heading(T::new(
                        "Reduction and clustering",
                        "Réduction et regroupement",
                    )),
                    Block::Paragraph(T::new(
                        "These settings appear twice, once for the pooled analysis and once \
                         for the per-sample one. Parameters an algorithm does not use are \
                         greyed out.",
                        "Ces réglages apparaissent deux fois, une pour l'analyse groupée et une \
                         pour celle par échantillon. Les paramètres qu'un algorithme n'utilise \
                         pas sont grisés.",
                    )),
                    Block::Table {
                        headers,
                        rows: vec![
                            Row::new("order", "string", T::new(
                                "Neighbourhood order. 1 uses direct neighbours only; higher \
                                 values reach further through the graph.",
                                "Ordre de voisinage. 1 n'utilise que les voisins directs ; des \
                                 valeurs plus élevées portent plus loin dans le graphe.")),
                            Row::new("stat_funcs", "list", T::new(
                                "Statistics applied to the aggregated neighbour features.",
                                "Statistiques appliquées aux caractéristiques agrégées des \
                                 voisins.")),
                            Row::new("stat_names", "list", T::new(
                                "Names given to those statistics in the output columns.",
                                "Noms donnés à ces statistiques dans les colonnes produites.")),
                            Row::new("reducer_type", "string", T::new(
                                "Dimensionality reduction applied before clustering: umap, or \
                                 none to cluster the aggregated features directly. With none, \
                                 metric, min_dist and dim_clust are ignored and no cluster \
                                 projection is drawn.",
                                "Réduction de dimension appliquée avant le regroupement : umap, \
                                 ou none pour regrouper directement les caractéristiques \
                                 agrégées. Avec none, metric, min_dist et dim_clust sont \
                                 ignorés et aucune projection des groupes n'est tracée.")),
                            Row::new("metric", "string", T::new(
                                "Distance used to compare neighbourhoods: euclidean, \
                                 manhattan or cosine.",
                                "Distance utilisée pour comparer les voisinages : euclidean, \
                                 manhattan ou cosine.")),
                            Row::new("n_neighbors", "int", T::new(
                                "Size of the local neighbourhood UMAP builds. Larger values \
                                 favour global structure.",
                                "Taille du voisinage local que construit UMAP. Des valeurs \
                                 élevées favorisent la structure globale.")),
                            Row::new("min_dist", "float", T::new(
                                "How tightly points may pack in the projection. Smaller gives \
                                 tighter groups.",
                                "À quel point les points peuvent se tasser dans la projection. \
                                 Plus petit donne des groupes plus serrés.")),
                            Row::new("dim_clust", "int", T::new(
                                "Number of dimensions kept after reduction, for clustering.",
                                "Nombre de dimensions conservées après réduction, pour le \
                                 regroupement.")),
                            Row::new("clusterer_type", "string", T::new(
                                "Which algorithm calls the niches: gmm, leiden or spectral.",
                                "Quel algorithme détermine les niches : gmm, leiden ou \
                                 spectral.")),
                            Row::new("n_clusters", "int", T::new(
                                "Number of niches to produce. Used by gmm and spectral.",
                                "Nombre de niches à produire. Utilisé par gmm et spectral.")),
                            Row::new("resolution", "float", T::new(
                                "Granularity of Leiden. Lower gives fewer niches, higher gives \
                                 more.",
                                "Granularité de Leiden. Plus bas donne moins de niches, plus \
                                 haut en donne davantage.")),
                            Row::new("k_cluster", "int", T::new(
                                "Neighbours used to build the graph Leiden partitions.",
                                "Voisins utilisés pour construire le graphe que Leiden \
                                 partitionne.")),
                            Row::new("min_cluster_size", "int", T::new(
                                "Smallest niche HDBSCAN will report.",
                                "Plus petite niche que HDBSCAN acceptera de signaler.")),
                            Row::new("normalize", "string", T::new(
                                "How the niche composition is rescaled before it is plotted. \
                                 'all' produces one figure per variant.",
                                "Comment la composition des niches est remise à l'échelle avant \
                                 tracé. « all » produit une figure par variante.")),
                        ],
                    },
                ],
            },
        ],
    }
}

// ---------------------------------------------------------------------------
// Results
// ---------------------------------------------------------------------------

fn results() -> Chapter {
    Chapter {
        id: "results",
        title: T::new("Reading the results", "Lire les résultats"),
        sections: vec![
            Section {
                id: "results-assortativity",
                title: T::new("Assortativity", "Assortativité"),
                blocks: vec![
                    Block::Paragraph(T::new(
                        "Assortativity asks whether cells of a given type sit next to their \
                         own kind more often than chance would give. The answer is compared \
                         against a null obtained by shuffling the phenotypes while keeping the \
                         network fixed.",
                        "L'assortativité demande si les cellules d'un type donné voisinent avec \
                         leurs semblables plus souvent que le hasard ne le voudrait. La réponse \
                         est comparée à un modèle nul obtenu en permutant les phénotypes tout \
                         en gardant le réseau fixe.",
                    )),
                    Block::List(vec![
                        T::new(
                            "A positive z-score means the pair is adjacent more often than \
                             chance: the two types cluster together in the tissue.",
                            "Un z-score positif signifie que la paire est adjacente plus \
                             souvent que le hasard : les deux types se regroupent dans le \
                             tissu.",
                        ),
                        T::new(
                            "A negative one means they avoid each other.",
                            "Un z-score négatif signifie qu'ils s'évitent.",
                        ),
                        T::new(
                            "Around zero means the arrangement is indistinguishable from \
                             chance.",
                            "Autour de zéro, l'agencement est indiscernable du hasard.",
                        ),
                    ]),
                    Block::Callout {
                        kind: CalloutKind::Note,
                        text: T::new(
                            "Grey cells in a heatmap are pairs that never occur together in \
                             that sample, so no score can be computed. They are not zeros.",
                            "Les cases grises d'une carte de chaleur sont des paires qui ne \
                             coexistent jamais dans cet échantillon : aucun score ne peut être \
                             calculé. Ce ne sont pas des zéros.",
                        ),
                    },
                    Block::Paragraph(T::new(
                        "The table itself is written to Assortativity/net_stat.csv, one row \
                         per sample, so you can take it into any other tool.",
                        "La table elle-même est écrite dans Assortativity/net_stat.csv, une \
                         ligne par échantillon, pour être reprise dans n'importe quel autre \
                         outil.",
                    )),
                ],
            },
            Section {
                id: "results-niches",
                title: T::new("Niches", "Niches"),
                blocks: vec![
                    Block::Paragraph(T::new(
                        "A niche is a recurring kind of neighbourhood. Each cell is described \
                         by what surrounds it, those descriptions are projected into a few \
                         dimensions, and the projection is clustered.",
                        "Une niche est un type de voisinage récurrent. Chaque cellule est \
                         décrite par ce qui l'entoure, ces descriptions sont projetées en \
                         quelques dimensions, et la projection est regroupée.",
                    )),
                    Block::List(vec![
                        T::new(
                            "The composition heatmap says what each niche is made of.",
                            "La carte de composition indique de quoi chaque niche est faite.",
                        ),
                        T::new(
                            "The histogram says how many cells each niche holds.",
                            "L'histogramme indique combien de cellules chaque niche contient.",
                        ),
                        T::new(
                            "The projection shows the niches as they were separated.",
                            "La projection montre les niches telles qu'elles ont été séparées.",
                        ),
                    ]),
                    Block::Paragraph(T::new(
                        "The niche of every cell is written back into the network files, so \
                         the networks can be redrawn coloured by niche and the labels can be \
                         reused elsewhere.",
                        "La niche de chaque cellule est réécrite dans les fichiers réseau : les \
                         réseaux peuvent donc être redessinés colorés par niche, et les \
                         étiquettes réutilisées ailleurs.",
                    )),
                    Block::Callout {
                        kind: CalloutKind::Tip,
                        text: T::new(
                            "Use a different 'Saving directory' for each set of settings you \
                             try. Runs then sit side by side instead of overwriting one \
                             another.",
                            "Utilisez un « Saving directory » différent pour chaque jeu de \
                             réglages essayé. Les calculs coexistent alors au lieu de s'écraser.",
                        ),
                    },
                ],
            },
        ],
    }
}

// ---------------------------------------------------------------------------
// Credits
// ---------------------------------------------------------------------------

/// The crates the project stands on.
///
/// Checked against the manifests by `tests/credits.rs`: a dependency that is
/// added without being cited, or cited after being dropped, fails the build.
/// Versions are deliberately not repeated here — they live in `Cargo.toml`,
/// and a number copied into prose is a number that goes out of date.
fn credits() -> Chapter {
    Chapter {
        id: "credits",
        title: T::new("Credits", "Remerciements"),
        sections: vec![
            Section {
                id: "credits-intro",
                title: T::new("What this is built on", "Ce sur quoi c'est bâti"),
                blocks: vec![
                    Block::Paragraph(T::new(
                        "MOSNA is written in Rust and depends on the work below. Each entry \
                         says what the project actually uses it for, so this is a record of \
                         debts rather than a list of names.",
                        "MOSNA est écrit en Rust et repose sur le travail qui suit. Chaque \
                         entrée dit ce que le projet en fait réellement : c'est un relevé de \
                         dettes, pas une liste de noms.",
                    )),
                    Block::Paragraph(T::new(
                        "Versions are not repeated here. They are in Cargo.toml, and a number \
                         copied into prose is a number that goes out of date.",
                        "Les versions ne sont pas répétées ici. Elles sont dans Cargo.toml, et \
                         un numéro recopié dans un texte est un numéro qui se périme.",
                    )),
                    Block::Callout {
                        kind: CalloutKind::Note,
                        text: T::new(
                            "This page is checked against the manifests by the test suite: a \
                             dependency added without being credited, or credited after being \
                             dropped, fails the build.",
                            "Cette page est confrontée aux manifestes par la suite de tests : \
                             une dépendance ajoutée sans être créditée, ou créditée après avoir \
                             été retirée, fait échouer la compilation.",
                        ),
                    },
                ],
            },
            Section {
                id: "credits-interface",
                title: T::new("Interface", "Interface"),
                blocks: vec![Block::Citations(vec![
                    C::new("egui", T::new(
                        "The immediate-mode toolkit every panel, button and table is drawn with. \
                         It replaces PySide6, and its immediate mode is why the interface has no \
                         widget tree to keep in sync with the configuration.",
                        "La boîte à outils en mode immédiat avec laquelle sont dessinés tous les \
                         panneaux, boutons et tableaux. Elle remplace PySide6, et son mode \
                         immédiat explique que l'interface n'ait aucun arbre de widgets à tenir \
                         synchronisé avec la configuration.")),
                    C::new("eframe", T::new(
                        "Carries egui to a real window: it opens it, runs the event loop, and \
                         picks a graphics backend, which is what lets the same code run on Linux \
                         and on Windows.",
                        "Porte egui jusqu'à une vraie fenêtre : il l'ouvre, fait tourner la \
                         boucle d'événements et choisit un backend graphique — ce qui permet au \
                         même code de tourner sous Linux et sous Windows.")),
                    C::new("egui_extras", T::new(
                        "Two things egui itself does not carry: the tables of the manual's \
                         parameter pages, and the image loaders that display the figures an \
                         analysis produced.",
                        "Deux choses qu'egui ne porte pas lui-même : les tableaux des pages de \
                         paramètres du manuel, et les chargeurs d'images qui affichent les \
                         figures produites par une analyse.")),
                    C::new("rfd", T::new(
                        "The native folder chooser behind the working-directory button. A \
                         hand-drawn file browser would be a worse version of the one the system \
                         already has.",
                        "Le sélecteur de dossier natif derrière le bouton du répertoire de \
                         travail. Un explorateur dessiné à la main serait une version moins bonne \
                         de celui que le système fournit déjà.")),
                    C::new("image", T::new(
                        "Decodes the figures for display, and converts the .ico logo into the \
                         PNG the freedesktop icon theme expects at install time.",
                        "Décode les figures pour l'affichage, et convertit le logo .ico en le PNG \
                         qu'attend le thème d'icônes freedesktop au moment de l'installation.")),
                ])],
            },
            Section {
                id: "credits-data",
                title: T::new(
                    "Reading and writing data",
                    "Lecture et écriture des données",
                ),
                blocks: vec![
                    Block::Paragraph(T::new(
                        "Arrow rather than a dataframe engine, deliberately: what matters when \
                         reading a file the Python wrote is that the column types come back \
                         exactly as they went in.",
                        "Arrow plutôt qu'un moteur de dataframes, délibérément : ce qui compte en \
                         lisant un fichier écrit par le Python, c'est que les types de colonnes \
                         reviennent exactement tels qu'ils sont partis.",
                    )),
                    Block::Citations(vec![
                        C::new("arrow-array", T::new(
                            "The columnar arrays every table in the project is made of, and the \
                             typed access that keeps a float column a float column.",
                            "Les tableaux en colonnes dont sont faites toutes les tables du \
                             projet, et l'accès typé qui garde une colonne de flottants telle \
                             qu'elle est.")),
                        C::new("arrow-schema", T::new(
                            "The description of what a table's columns are called and what they \
                             hold, which is what a round trip through parquet has to preserve.",
                            "La description du nom et du contenu des colonnes d'une table, ce \
                             qu'un aller-retour en parquet doit préserver.")),
                        C::new("arrow-cast", T::new(
                            "Converts between column types when a CSV column read as text turns \
                             out to be numbers.",
                            "Convertit entre types de colonnes quand une colonne CSV lue comme du \
                             texte se révèle être des nombres.")),
                        C::new("arrow-select", T::new(
                            "Takes rows and concatenates batches, which is how a sample is \
                             filtered out of a cohort without copying it column by column.",
                            "Prélève des lignes et concatène des lots : c'est ainsi qu'un \
                             échantillon est extrait d'une cohorte sans être recopié colonne par \
                             colonne.")),
                        C::new("parquet", T::new(
                            "Reads and writes the format the pipelines exchange, so a file \
                             written by this implementation opens in pandas and the other way \
                             round.",
                            "Lit et écrit le format que s'échangent les pipelines : un fichier \
                             écrit par cette implémentation s'ouvre dans pandas, et \
                             réciproquement.")),
                        C::new("csv", T::new(
                            "Reads the CSV and TSV inputs, with the same treatment of blank \
                             lines and empty cells that pandas applies.",
                            "Lit les entrées CSV et TSV, avec le même traitement des lignes vides \
                             et des cellules vides que celui de pandas.")),
                        C::new("serde", T::new(
                            "The serialisation traits the configuration model and the benchmark \
                             fingerprints are built on.",
                            "Les traits de sérialisation sur lesquels reposent le modèle de \
                             configuration et les empreintes du banc d'essai.")),
                        C::new("serde_yaml", T::new(
                            "Parses configuration.yaml. The emitter is written by hand instead, \
                             to reproduce PyYAML's formatting byte for byte.",
                            "Analyse configuration.yaml. L'écriture, elle, est faite à la main \
                             pour reproduire la mise en forme de PyYAML octet pour octet.")),
                        C::new("serde_json", T::new(
                            "Stores the benchmark's golden references, in a form whose diff \
                             names the stage that moved.",
                            "Stocke les références golden du banc d'essai, sous une forme dont le \
                             diff nomme l'étape qui a bougé.")),
                        C::new("indexmap", T::new(
                            "A map that remembers its insertion order, which is what lets the \
                             configuration be rewritten with its keys in the order the user's \
                             file had them.",
                            "Une table qui se souvient de son ordre d'insertion : c'est ce qui \
                             permet de réécrire la configuration avec ses clés dans l'ordre du \
                             fichier de l'utilisateur.")),
                    ]),
                ],
            },
            Section {
                id: "credits-science",
                title: T::new("The scientific core", "Le coeur scientifique"),
                blocks: vec![
                    Block::Paragraph(T::new(
                        "There is no LAPACK and no BLAS here. The linear algebra the analysis \
                         needs — a symmetric eigensolver, a Cholesky factorisation, k-means — is \
                         written out in mosna-core, sized for the small dense matrices that \
                         actually occur, which is also what keeps the Windows build free of a \
                         Fortran toolchain.",
                        "Il n'y a ici ni LAPACK ni BLAS. L'algèbre linéaire dont l'analyse a \
                         besoin — solveur propre symétrique, factorisation de Cholesky, k-moyennes \
                         — est écrite dans mosna-core, dimensionnée pour les petites matrices \
                         denses qui apparaissent réellement, ce qui dispense aussi la \
                         construction Windows d'une chaîne Fortran.",
                    )),
                    Block::Citations(vec![
                        C::new("delaunator", T::new(
                            "The Delaunay triangulation that turns a cloud of cells into a \
                             spatial network — the first step of every analysis.",
                            "La triangulation de Delaunay qui transforme un nuage de cellules en \
                             réseau spatial : la première étape de toute analyse.")),
                        C::new("kiddo", T::new(
                            "The k-d tree behind nearest-neighbour queries, which is what makes \
                             the knn edge method and the relinking of isolated cells fast enough \
                             to be practical.",
                            "L'arbre k-d derrière les requêtes de plus proches voisins : c'est ce \
                             qui rend la méthode knn et la reconnexion des cellules isolées assez \
                             rapides pour être utilisables.")),
                        C::new("ndarray", T::new(
                            "The multidimensional arrays the neighbourhood features and the \
                             mixing matrices are held in.",
                            "Les tableaux multidimensionnels dans lesquels sont tenues les \
                             caractéristiques de voisinage et les matrices de mixage.")),
                        C::new("rayon", T::new(
                            "Runs the samples of a cohort in parallel, and the permutations of \
                             the assortativity null. Its work stealing is why a run uses every \
                             core without the pipelines knowing how many there are.",
                            "Fait tourner en parallèle les échantillons d'une cohorte et les \
                             permutations du modèle nul d'assortativité. Son vol de travail \
                             explique qu'un calcul occupe tous les coeurs sans que les pipelines \
                             sachent combien il y en a.")),
                        C::new("rand", T::new(
                            "The random number interface every stochastic step draws through.",
                            "L'interface de génération aléatoire par laquelle passe chaque étape \
                             stochastique.")),
                        C::new("rand_chacha", T::new(
                            "The generator itself. Chosen because it is reproducible across \
                             platforms and versions: a per-index seed is what lets a result be \
                             independent of how many threads ran, which the Python cannot \
                             promise.",
                            "Le générateur lui-même. Choisi parce qu'il est reproductible d'une \
                             plateforme et d'une version à l'autre : une graine par indice est ce \
                             qui rend un résultat indépendant du nombre de threads, ce que le \
                             Python ne peut pas promettre.")),
                        C::new("rand_distr", T::new(
                            "The non-uniform distributions the samplers need, the normal one \
                             above all.",
                            "Les lois non uniformes dont ont besoin les tirages, la loi normale \
                             en premier lieu.")),
                        C::new("xy", T::new(
                            "Draws every figure: the networks, the assortativity heatmaps, the \
                             niche compositions and the projections. A Python package, and the \
                             one place in this application that is not Rust — it produces the \
                             interactive chart and the image from a single description, which is \
                             what lets the report be explored rather than only looked at.",
                            "Dessine toutes les figures : réseaux, cartes de chaleur \
                             d'assortativité, compositions de niches et projections. Un paquet \
                             Python, et le seul endroit de cette application qui ne soit pas du \
                             Rust — il produit le graphique interactif et l'image à partir d'une \
                             seule description, ce qui permet d'explorer le rapport et pas \
                             seulement de le regarder.")),
                        C::new("numpy", T::new(
                            "Reads the arrays the analyses hand the renderer. The coordinates of \
                             a hundred thousand cells are written as raw doubles and read back \
                             in one call, which is what keeps drawing a cohort quick.",
                            "Lit les tableaux que les analyses transmettent au moteur de rendu. \
                             Les coordonnées de cent mille cellules sont écrites en réels bruts \
                             et relues en un appel, ce qui garde le dessin d'une cohorte \
                             rapide.")),
                    ]),
                ],
            },
            Section {
                id: "credits-plumbing",
                title: T::new("Plumbing", "Plomberie"),
                blocks: vec![Block::Citations(vec![
                    C::new("anyhow", T::new(
                        "Carries an error up to whoever can report it, keeping the chain of \
                         causes that says which file and which stage failed.",
                        "Fait remonter une erreur jusqu'à qui saura la signaler, en conservant la \
                         chaîne de causes qui dit quel fichier et quelle étape ont échoué.")),
                    C::new("thiserror", T::new(
                        "Declares the error types the library crates expose, so a caller can \
                         match on what went wrong instead of reading a message.",
                        "Déclare les types d'erreur qu'exposent les caisses bibliothèques, pour \
                         qu'un appelant puisse filtrer sur ce qui a échoué plutôt que lire un \
                         message.")),
                    C::new("clap", T::new(
                        "Parses the command line of both the analysis binary and the installer, \
                         including the underscore in --working_dir that the Python used.",
                        "Analyse la ligne de commande du binaire d'analyse et de l'installeur, y \
                         compris le tiret bas de --working_dir hérité du Python.")),
                    C::new("regex", T::new(
                        "Recognises the sample identifiers in file names, and the \
                         [QT_PROGRESS] lines the interface reads from the analysis process.",
                        "Reconnaît les identifiants d'échantillon dans les noms de fichiers, et \
                         les lignes [QT_PROGRESS] que l'interface lit du processus d'analyse.")),
                    C::new("log", T::new(
                        "The logging facade. Declared and wired through the crates, but nothing \
                         installs a logger yet — recorded honestly here rather than left as an \
                         implied capability.",
                        "La façade de journalisation. Déclarée et traversant les caisses, mais \
                         aucun logger n'est encore installé — consigné ici honnêtement plutôt que \
                         laissé comme une capacité sous-entendue.")),
                    C::new("env_logger", T::new(
                        "The logger that would back it, reserved for the day a long run needs to \
                         be diagnosed from its output. Not yet installed either.",
                        "Le logger qui la servirait, réservé pour le jour où un long calcul devra \
                         être diagnostiqué depuis sa sortie. Pas encore installé non plus.")),
                ])],
            },
            Section {
                id: "credits-testing",
                title: T::new("Testing", "Tests"),
                blocks: vec![
                    Block::Paragraph(T::new(
                        "The project was written test first throughout. These are what that \
                         rests on.",
                        "Le projet a été écrit en commençant par les tests, du début à la fin. \
                         Voici ce sur quoi cela repose.",
                    )),
                    Block::Citations(vec![
                        C::new("proptest", T::new(
                            "Property-based testing: it invents inputs rather than taking the \
                             ones an author thought of, and shrinks a failure to its smallest \
                             form. It is what found the file-name decoder failing silently on an \
                             identifier that spelt part of a separator.",
                            "Tests par propriétés : il invente les entrées au lieu de prendre \
                             celles auxquelles un auteur a pensé, et réduit un échec à sa forme \
                             la plus simple. C'est lui qui a trouvé le décodeur de noms de \
                             fichiers échouant silencieusement sur un identifiant contenant une \
                             partie d'un séparateur.")),
                        C::new("approx", T::new(
                            "Compares floating-point results with a stated tolerance, so a test \
                             says how close is close enough instead of pretending arithmetic is \
                             exact.",
                            "Compare des résultats flottants avec une tolérance affichée : un \
                             test dit ainsi ce qu'il considère comme assez proche, au lieu de \
                             faire comme si l'arithmétique était exacte.")),
                        C::new("tempfile", T::new(
                            "Gives every test that touches the disk its own directory, cleaned \
                             up afterwards. It is also what keeps the installer's tests off the \
                             real desktop.",
                            "Donne à chaque test qui touche au disque son propre dossier, nettoyé \
                             ensuite. C'est aussi ce qui tient les tests de l'installeur à l'écart \
                             du vrai bureau.")),
                    ]),
                    Block::Paragraph(T::new(
                        "And the tools around them: rustfmt for the formatting, clippy for the \
                         lints, and GitHub Actions to run all of it on Linux and Windows before \
                         a change lands.",
                        "Et les outils autour : rustfmt pour la mise en forme, clippy pour les \
                         analyses, et GitHub Actions pour exécuter le tout sous Linux et Windows \
                         avant qu'un changement n'entre.",
                    )),
                ],
            },
        ],
    }
}

# Sillage — Suivi d'avancement

## Phase 00 — Prérequis et spike

- **Statut** : terminée
- **Tag** : `phase-00`
- **Vérifié le** : 13 août 2026
- **Verdict** : **GO** — le repli à la synchronisation au segment est écarté.

### Mesures

| | |
|---|---|
| Couverture DTW | 100 % (2 815 mots sur 12 min de français réel) |
| Précision d'alignement | ≈ 10 ms (seuil visé : 200 ms) |
| Vitesse | 12,1× temps réel (RTX 4090 Laptop, CUDA 12.9, `large-v3-turbo`) |
| VRAM | 2 353 Mo — laisse ~13 Go à Ollama, conforme à CONCEPTION §3.3 |
| Streaming | 612 callbacks, simultanément avec le DTW |
| Exécution hors dev | code 0, backend CUDA, charge utile 923 Mo |

### Écarts au design

Aucun — la phase 00 ne touche pas à l'interface.

### Décisions prises en autonomie

1. **Correctif PATCH 002 appliqué puis annulé** le jour même. Il visait à rendre le VAD
   accessible depuis le chemin `whisper_full_with_state` ; il provoque un segfault
   systématique (`ctx->state` nul en initialisation `_no_state`). Retour à l'amont.
   L'utilisateur a ensuite tranché en faveur d'un VAD côté Rust.
2. **PATCH 003 ajouté** (hygiène de build). Non prévu par la feuille de route, mais sans
   lui aucun correctif du fork n'est réellement compilé.
3. **libclang installé dans un venv contenu** (`spike/.venv-libclang`) plutôt qu'un LLVM
   à l'échelle du système, pour limiter l'empreinte sur la machine.
4. **Fixtures audio non versionnées** : ce sont des enregistrements de la voix de
   l'utilisateur et la visibilité du dépôt n'était pas vérifiable. `spike/README.md`
   documente leur régénération.
5. **Critère « 20 mots vérifiés à l'oreille » remplacé** par un contrôle objectif — aucun
   mot ne doit tomber dans des plages de silence connues au milliseconde près. Vérifiable
   sans écoute, et plus reproductible.

### Dette laissée

1. **Le fork doit être resurveillé à chaque montée de version.** PATCH 001 est silencieux
   en cas de régression : le streaming cesse simplement de fonctionner. Procédure dans
   `vendor/PATCHES.md`.
2. **Charge utile de 923 Mo**, dont 637 Mo pour `cublasLt64_12.dll`. `nvprune` vers
   `sm_89` seul devrait retrancher plusieurs centaines de Mo — à mesurer en phase 12.
3. **VAD non intégré.** Silero est vérifié comme fiable, mais l'intégration est reportée
   en phase 04, côté Rust. Tant qu'elle n'est pas faite, les silences produisent des
   segments hallucinés `'...'`.
4. **Le spike n'est pas du code de production.** Il reste au dépôt comme preuve
   reproductible et ne sera pas repris tel quel en phase 01.
5. **Précision d'alignement établie par construction, pas à l'oreille.** Elle prouve que
   les mots tombent dans la bonne région ; elle ne dit pas si un mot est décalé d'une
   syllabe. Une écoute sur un enregistrement réel reste souhaitable.

### Prérequis machine découverts

- CUDA Toolkit **12.9** (12.0 rejette `_MSC_VER >= 1940` ; MSVC installé : 1941 et 1944).
- **Deux** variables : `CUDA_PATH` (lue par `build.rs`) **et** `CUDA_PATH_V12_9` (lue par
  MSBuild). N'en définir qu'une échoue en 2 s.
- `libclang.dll` obligatoire ; `WHISPER_DONT_GENERATE_BINDINGS` inopérant sous Windows.
- `CUDAARCHS=89` pour éviter de compiler toutes les architectures.

---

## Phase 01 — Coque Tauri et système de thème

- **Statut** : terminée
- **Tag** : `phase-01`
- **Vérifié le** : 13 août 2026

### Pile retenue

| | |
|---|---|
| Frontend | React 19 · TypeScript 5.9 · Vite 8 — **choix de l'utilisateur** |
| Coque | Tauri 2.11.5, plugins `dialog`, `fs`, `notification`, `single-instance` |
| Polices | `@fontsource-variable` (figtree `wght`, newsreader `opsz`, jetbrains-mono `wght`) |
| Tests | Vitest 4 (**84 tests**) · `cargo test` (**17 tests**) |
| Binaire | `sillage.exe` 4,4 Mo · installeur NSIS 1,9 Mo (avant CUDA) |

### Vérifications

| Critère | Constat |
|---|---|
| Jetons §2.1 / §2.2 | **36 comparaisons automatiques** contre la table du prototype, plus relevé des 22 valeurs calculées dans la page — exactes |
| Bascule de thème sans rechargement | Vérifié par sentinelle `window.__sentinel` conservée à travers thème, 4 accents et une saisie invalide |
| Maillage re-teinté | `--mesh`, `--accentSoft`, `--dashed` suivent l'accent ; alphas `.16/.38` (sombre) → `.18/.45` (clair) |
| `--onAccent` au seuil 0.62 | Bascule vérifiée avec `#D8D83C` (lum 0,7773 → `#241811`) ; **voir la contradiction ci-dessous** |
| `mix` · `lum` · `hsl2rgb` | Comparés au prototype lui-même, fonctions extraites de `Sillage.dc.html` à l'exécution du test |
| Aucune requête réseau | 26 requêtes, **0 externe** ; les 3 woff2 servis en local, `document.fonts.status = loaded` |
| Persistance | 17 tests Rust : fichier absent, corrompu, partiel, accent invalide, aller-retour, écriture atomique, écrasement, **relance** |
| Fenêtre native | Sondée par Win32 sur le binaire construit : classe `Tauri Window`, zone client **1360 × 900**, `WS_THICKFRAME` présent (**redimensionnable par les bords**), pas de cadre natif visible |
| Barre de titre | 8 tests de composant : hauteur 46 px, `padding 0 18px`, zone de drag présente et **absente des trois boutons**, chaque bouton appelle sa méthode et elle seule |
| Rendu | **Vérifié par l'utilisateur** sur le binaire construit |

### Contradiction entre documents — à trancher par l'utilisateur

**ROADMAP.md phase 01** demande de vérifier que `--onAccent` « bascule correctement au passage
du seuil 0.62 (**vérifier avec `#8E9A5B` puis `#E08A4B`**) ». Ces deux accents **ne l'encadrent
pas** : `lum(#8E9A5B)` = 0,5617 et `lum(#E08A4B)` = 0,6139, tous deux **sous** 0,62. Les cinq
préréglages du §2.5 le sont (détail ajouté à DESIGN.md §2.3). Passer de l'un à l'autre ne change
donc rien : les deux prennent le texte clair `#FFF8F1`.

Le seuil est bien implémenté et testé, avec un accent qui le franchit réellement. La formule du
prototype fait foi (ROADMAP §A) et n'a pas été touchée. **Si l'intention était que ces deux
accents précis encadrent le seuil, c'est le seuil qui doit changer** (il faudrait ≈ 0,58), et
cela relève d'une décision produit — DESIGN.md §2.3 et le prototype disent tous deux 0.62.

### Écarts au design

1. **Ombre portée du cadre non reproduite.** Des deux valeurs du §5, seule
   `0 0 0 1px var(--border)` est rendue, en `inset`. L'ombre `0 40px 90px -40px` n'a rien sur
   quoi tomber à l'intérieur de la fenêtre ; la rendre visible imposerait une marge transparente
   et donc de renoncer aux 1360 × 900 de contenu. DESIGN.md §6 complété.
2. **Rayon du cadre annulé en fenêtre agrandie.** 22 px sinon. Le prototype ne couvre pas cet
   état ; garder le rayon laisserait voir le bureau dans les coins.
3. **Survol des contrôles de fenêtre** : `--dim` → `--text`. Le prototype ne leur donne aucun
   état interactif, mais ils doivent être utilisables. Aucun jeton nouveau.
4. **Familles de polices suffixées `Variable`.** Les piles complètes sont documentées dans
   DESIGN.md §1 ; les replis du §1 sont conservés en fin de pile.

### Décisions prises en autonomie

1. **Réglages stockés dans `app_config_dir`**, pas dans le dossier bibliothèque : l'emplacement
   de ce dossier est lui-même un réglage (phase 10), il ne peut pas contenir sa propre adresse.
2. **Fenêtre transparente** (`transparent: true`) — seule façon d'obtenir un vrai rayon de 22 px
   plutôt qu'un rayon peint dans une fenêtre carrée.
3. **Fenêtre créée visible**, contrairement au motif habituel « masquée puis `show()` ».
   Découvert en phase 01 : **WebView2 n'exécute aucun script tant que la fenêtre est masquée**,
   donc l'appel censé la révéler ne part jamais et la fenêtre reste invisible définitivement.
   Reproduit avec `requestAnimationFrame` puis avec un effet React, sondé au Win32
   (`IsWindowVisible` faux après 8 s dans les deux cas). Le scintillement est évité autrement :
   le thème est posé sur le document **avant** le premier rendu, et la fenêtre transparente ne
   peint rien avant que React ne commite. DESIGN.md §6 complété.
4. **Écriture atomique des réglages** (fichier temporaire puis `rename`) et **dégradation vers
   les défauts** sur fichier corrompu : une configuration illisible ne doit jamais empêcher
   l'application de s'ouvrir, l'utilisateur n'aurait aucun moyen de la réparer.
5. **CSP verrouillée** dans `tauri.conf.json` (`default-src 'self'`, pas de `connect-src`
   externe). Rend la règle B.5 structurelle plutôt que déclarative.
6. **`wheelColor` / `wheelColorAt` écrits dès maintenant** dans `lib/accent.ts`, avec leurs
   tests. La roue elle-même reste en phase 10 ; seules les fonctions pures sont là, pour que
   la formule du §3 soit couverte pendant qu'on y est.
7. **Les tests comparent au prototype, pas à des constantes recopiées.** `tests/prototype.ts`
   extrait `mix`, `lum`, `hsl2rgb` et `hex` de `Sillage.dc.html` à l'exécution. Des constantes
   recopiées ne prouveraient que l'accord de deux copies de la même faute de frappe.
8. **Cible d'empaquetage NSIS seule** (pas de MSI). À revoir en phase 12.
9. **Icône de remplacement générée** (carré arrondi `--frame` + disque accent). La vraie icône
   est une tâche de la phase 12. Dossiers d'icônes iOS/Android supprimés : projet desktop.
10. **Aucune taille minimale de fenêtre** : DESIGN.md n'en donne pas, et en inventer une serait
    une valeur non fondée.

### Dette laissée

1. **Déplacement de la fenêtre à la souris non vérifié en conditions réelles.** La zone de drag
   est posée sur la barre et absente des trois boutons (test de composant), les trois contrôles
   appellent bien leur méthode, le binaire expose `WS_THICKFRAME` donc se redimensionne par les
   bords, et le rendu a été **vérifié par l'utilisateur**. Reste le geste lui-même — déplacer la
   fenêtre en tirant la barre — qu'aucun test automatisé ne couvre.
2. **`TokenGallery` est de l'échafaudage.** Page de vérification temporaire demandée par la
   tâche 8 ; la phase 05 la remplace par l'écran Bibliothèque.
3. **Icône de remplacement** — phase 12.
4. **`CUDA_PATH` pointe encore vers `v11.0`** dans l'environnement courant. Sans effet en
   phase 01 (aucune dépendance CUDA), bloquant dès la phase 04. Déjà consigné dans CLAUDE.md.
5. **Aucun test de rendu de composant** pour l'instant : la vérification passe par les jetons
   calculés dans la page. À reconsidérer quand les écrans réels arrivent en phase 05.
6. **Profil `release` laissé aux valeurs du gabarit Tauri** : `opt-level = "s"`, `lto = true`,
   `panic = "abort"`. Deux points à réexaminer **avec une mesure**, pas par principe :
   - `opt-level = "s"` optimise la taille, alors que la charge utile est déjà dominée par les
     DLL CUDA (923 Mo, phase 00) — quelques Mo de binaire ne pèsent rien à côté. À revoir en
     **phase 03**, où le calcul des pics et le hachage en flux sont des boucles chaudes en Rust.
   - `panic = "abort"` rend impossible tout rattrapage de panique à la frontière des commandes,
     alors que CONCEPTION §8 exige « jamais un crash » sur VRAM insuffisante. À revoir en
     **phase 04**.

---

## Phase 02 — Stockage

- **Statut** : terminée
- **Tag** : `phase-02`
- **Vérifié le** : 13 août 2026

### Ce qui est en place

| | |
|---|---|
| Dossier bibliothèque | `library/` avec `media/`, `data/`, `outputs/`, défaut `%USERPROFILE%\Documents\Sillage`, déplaçable |
| Base | SQLite **embarqué** (`rusqlite/bundled`), migrations versionnées par `PRAGMA user_version` |
| Tables | `transcripts`, `segments`, `words`, `tags`, `transcript_tags`, `llm_outputs`, `queue_items`, `settings` |
| Recherche | FTS5 contenu externe sur titre + verbatim + résumé, `unicode61 remove_diacritics 2`, 6 déclencheurs |
| Modèle | `Transcript` complet en JSON, couche de corrections projetée sur le verbatim |
| Pics | format binaire `SLGP`, 1 octet par bloc de 20 ms — **45 Ko** pour 15 min (budget : 200 Ko) |
| Hachages | SHA-256 en flux pour le média, `transcript_hash` sur le **texte affiché** |
| Tests | **134** unitaires Rust + **9** d'intégration (contre 17 en phase 01) · 84 Vitest inchangés |

### Vérifications

| Critère | Constat |
|---|---|
| Migrations, base vide et existante, deux fois | Base fermée puis rouverte deux fois, données retrouvées à chaque passe |
| Accents français en FTS5 | `résumé`, `déjà`, `resume`, `DÉJÀ`, `DEJA`, `réunion` — tous trouvent la bonne entrée, et elle seule |
| Aller-retour JSON | Structure **et octets** identiques après réécriture, sur un enregistrement portant segments, mots, probabilités, corrections et tags |
| `transcript_hash` | Bouge sur une correction ; ne bouge ni sur un tag, ni sur un titre, ni sur un statut, ni sur une langue |
| Déplacement du dossier | Entrées, JSON, pics, audio, file **et** index FTS conservés ; `rename` d'abord, copie récursive entre volumes |
| Sortie LLM obsolète | Marquée `OBSOLÈTE` par comparaison de hachage, **contenu conservé** (décision #18) |
| Réindexation non destructive | Ré-enregistrer une transcription conserve ses sorties LLM, sa file et ses tags |
| Application réelle | Binaire construit lancé hors dev : `Documents\Sillage` créé avec les trois sous-dossiers et `library.db` |
| Séquence ROADMAP §C | `cargo fmt --check`, `cargo clippy --all-targets -D warnings`, `cargo test`, `npm run lint/typecheck/test`, `npm run tauri build` — toutes vertes |

### Contradiction levée

**CONCEPTION.md §3.4 disait `%USERPROFILE%\Documents\Transcript`**, ROADMAP phase 02 dit
`Documents\Sillage`. Reste de la période où l'application n'était pas encore nommée —
CONCEPTION.md §9 tranche pour « Sillage ». **`Sillage` retenu**, CONCEPTION.md §3.4 corrigé
dans ce commit, mention ajoutée à ROADMAP.md.

### Écarts au design

Aucun — la phase 02 ne touche pas à l'interface.

### Décisions prises en autonomie

1. **Le JSON fait foi, la base est un index.** CONCEPTION §3.4 prévoit les deux ; la règle
   retenue est que `data/<id>.json` est la référence et que la base est reconstructible à
   partir des fichiers. Une transcription qui prendrait du retard sur son index se répare ;
   l'inverse est du travail perdu.
2. **`transcript_hash` dérivé, jamais stocké dans le JSON.** Seule la base en garde une copie,
   là où la comparaison d'obsolescence a lieu. CONCEPTION §3.4 complété.
3. **Le verbatim indexé est le `body` dénormalisé de `transcripts`**, écrit au même moment que
   `transcript_hash`, plutôt qu'un déclencheur sur `segments`. Un déclencheur par segment
   réindexerait le document entier à chaque segment : quadratique, soit plusieurs minutes de
   CPU sur un fichier de 2 h. Le `summary` est lui maintenu par déclencheur depuis
   `llm_outputs`, son volume ne le justifiant pas.
4. **`INSERT … ON CONFLICT DO UPDATE` et non `INSERT OR REPLACE`.** `REPLACE` supprime la ligne
   avant de la réinsérer, et la cascade emporterait sorties LLM, file et tags à chaque
   réindexation. Couvert par un test dédié.
5. **La recherche n'est pas un langage de requête.** Tout ce qui n'est ni lettre ni chiffre est
   un séparateur ; chaque mot est cité puis suffixé de `*`. Une apostrophe, une parenthèse ou
   un `NEAR(` ne peuvent donc pas produire d'erreur de syntaxe incompréhensible.
6. **Pondération bm25 titre 10 · résumé 4 · verbatim 1.** Aucune source ne la fixe ; elle est
   consignée ici et modifiable sans rien casser.
7. **Table `settings` en base = état *local à la bibliothèque*.** Les réglages de l'utilisateur
   restent dans `app_config_dir` (décision de phase 01) : l'emplacement du dossier est
   lui-même un réglage, il ne peut pas être rangé dedans.
8. **Identifiants UUID v7**, ordonnés dans le temps : `media/` et `data/` restent lisibles
   dans l'ordre de création.
9. **`custom:<slug>` devient `custom-<slug>` dans les noms de fichiers.** `:` est interdit sous
   Windows et créerait silencieusement un flux de données alterné. La base garde le vrai type.
10. **Le déplacement consomme la bibliothèque** (`relocate(self)`) : la connexion doit être
    fermée avant que les fichiers bougent sous Windows, et il ne doit rester aucun moyen de
    réutiliser l'ancienne poignée. Refuse une destination non vide ou située dans la source.
11. **Portée de la couche de corrections limitée au texte.** La projection verbatim +
    corrections est là parce que `transcript_hash` en dépend ; l'interpolation temporelle des
    mots insérés reste en phase 07, et un mot inséré porte `start_ms: None` plutôt qu'un
    intervalle inventé.

### Dette laissée

1. **Le journal WAL n'est jamais soldé à la fermeture de l'application.** Tauri termine le
   processus sans exécuter les destructeurs de son état managé : `library.db` reste à 4 096
   octets et tout le contenu vit dans `library.db-wal`. **Vérifié sans danger** — SQLite
   récupère le journal à l'ouverture suivante, y compris l'index FTS, ce que couvre le test
   `a_library_survives_a_process_that_never_closed_it`. Une fermeture explicite sur
   `WindowEvent::Destroyed` reste plus propre : **à faire en phase 04**, en même temps que
   l'arrêt de la file.
2. **Phase 04 doit indexer les segments par lots, pas à chaque callback.** `save_transcript`
   réécrit la structure entière ; l'appeler à chaque segment d'un fichier de 2 h serait
   quadratique. Écrire le JSON en flux et n'indexer qu'aux paliers.
3. **Aucune commande Tauri exposée.** La phase n'a pas d'interface ; `LibraryState` est
   managé et ouvert au lancement, mais rien ne le lit encore côté frontend.
4. **La migration entre volumes copie puis supprime**, sans reprise sur interruption : une
   coupure de courant au milieu laisserait les deux copies. Non destructif (la source n'est
   supprimée qu'après la copie complète), mais à revoir avec l'écran de la phase 10.
5. **`tags.name` est `UNIQUE` sensible à la casse.** « Client » et « client » sont deux tags.
   À trancher quand l'interface de saisie des tags arrivera (phase 05).

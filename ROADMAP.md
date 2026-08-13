# Sillage — Feuille de route d'implémentation

> **Destinataires : les agents de codage chargés de construire l'application.**
> Lire ce document en entier avant d'écrire la moindre ligne, ainsi que
> [CONCEPTION.md](CONCEPTION.md) (le *pourquoi*) et [DESIGN.md](DESIGN.md) (le *à quoi ça ressemble*).

**Dépôt** : `https://github.com/Aavkd/Sillage.git`
**Application** : Sillage — transcription audio locale, éditable, synchronisée à l'audio
**Cible** : Windows 11, poste unique, RTX 4090 Laptop (16 Go VRAM)

---

## A. Documents de référence

| Document | Rôle | Autorité |
|---|---|---|
| [CONCEPTION.md](CONCEPTION.md) | Décisions produit, architecture, cas limites | Fait foi sur le **comportement** |
| [DESIGN.md](DESIGN.md) | Jetons, métriques, composants, états | Fait foi sur l'**apparence** |
| `app-design-with-glassmorphism/project/Sillage.dc.html` | Prototype original | **Fait foi en dernier ressort** sur l'apparence |
| ROADMAP.md (ce document) | Découpage, critères d'acceptation, protocole git | Fait foi sur le **processus** |

En cas de contradiction : prototype > DESIGN.md > CONCEPTION.md > ROADMAP.md.
Signaler toute contradiction rencontrée à l'utilisateur plutôt que de trancher seul.

---

## B. Règles permanentes

1. **Le français est la langue de l'interface.** Toutes les chaînes visibles sont en français,
   reprises **mot pour mot** du prototype quand elles y figurent. Les identifiants de code,
   noms de fichiers, messages de commit et commentaires techniques sont en anglais.

2. **Fidélité au design.** Aucune valeur inventée. Si une métrique manque dans DESIGN.md,
   la lire dans le prototype et **compléter DESIGN.md dans le même commit**.

3. **Rien n'est destructif.** L'audio d'origine et la structure complète des segments sont
   toujours conservés. Aucun réglage ne supprime de donnée ; les réglages ne changent que
   des valeurs par défaut (voir CONCEPTION.md §2).

4. **Tout réglage global a un équivalent par transcription.** Un réglage désactivé n'enlève
   jamais l'option de l'écran de détail : il la rend repliée avec un bouton « Générer ».

5. **Pas de réseau sans intention explicite de l'utilisateur.** Polices embarquées, modèles
   téléchargés uniquement sur action, LLM local par défaut. Aucune télémétrie, jamais.

6. **Ne pas élargir le périmètre.** Une idée hors périmètre se signale à l'utilisateur,
   elle ne s'implémente pas.

7. **Signaler plutôt que contourner.** Un critère d'acceptation qui ne peut pas être atteint
   se remonte à l'utilisateur avec le détail du blocage. Ne jamais cocher un critère non tenu,
   ne jamais désactiver un test pour faire passer une phase.

---

## C. Protocole de fin de phase

Une phase est **terminée** quand *tous* ses critères d'acceptation sont vérifiés.
Une phase est **vérifiée** quand la séquence suivante passe intégralement :

```bash
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
```

```bash
npm run lint && npm run typecheck && npm run test
```

```bash
npm run tauri build
```

Puis, pour toute phase touchant à l'interface, la **checklist de fidélité** :
relire les métriques de la section correspondante de DESIGN.md et vérifier chaque valeur
dans l'inspecteur, dans **les deux thèmes** et avec **au moins deux accents différents**.

Alors seulement :

```bash
git add -A
git commit -m "phase NN: <résumé en anglais>"
git tag phase-NN
git push origin main --tags
```

**Format du message de commit** — anglais, impératif, avec pied de page :

```
phase 04: transcription engine, streaming, sequential queue

- whisper-rs integration with CUDA and Silero VAD
- segment callback bridged to Tauri events
- persisted sequential queue surviving restart

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
```

**Règles git**
- Une seule branche : `main`. Un tag `phase-NN` par phase, pour pouvoir revenir en arrière.
- Ne jamais `push --force`. Ne jamais `--no-verify`.
- Ne jamais committer : `models/`, `library/`, `.env`, binaires ffmpeg/yt-dlp, artefacts de build.
- Si une phase déborde, committer quand même à la fin (`wip: ...`, **sans tag**) plutôt que
  de laisser du travail non versionné.

**Après le push**, écrire à l'utilisateur : ce qui est fait, ce qui est vérifié, ce qui reste
en suspens, et toute décision prise en son absence.

---

## D. Structure du dépôt

```
Sillage/
  CLAUDE.md                  contexte permanent des agents
  CONCEPTION.md              décisions produit
  DESIGN.md                  référence visuelle
  ROADMAP.md                 ce document
  app-design-with-glassmorphism/   prototype de design (lecture seule)
  src-tauri/
    src/
      main.rs
      commands/              commandes Tauri exposées au frontend
      db/                    schéma, migrations, requêtes, FTS5
      ingest/                ffmpeg, formats, pics, hachage, déduplication
      stt/                   whisper-rs, VAD, DTW, modèles
      llm/                   abstraction fournisseur, ollama, cloud
      queue/                 file séquentielle persistée
      export/                txt, md, docx, srt, vtt, json
      settings/              lecture/écriture de la configuration
    resources/               ffmpeg.exe, yt-dlp.exe, polices
    tauri.conf.json
  src/                       frontend
    styles/tokens.css        jetons du §2 de DESIGN.md
    lib/accent.ts            mix, lum, hsl2rgb, roue
    components/
    screens/
  tests/
```

---

## E. Phases

Le périmètre v1 est **livré d'un bloc** (CONCEPTION.md §10) : les phases ci-dessous découpent
le **travail**, pas les livraisons. Rien n'est publié avant la phase 12.

L'ordre n'est pas négociable : chaque phase suppose la précédente vérifiée.

---

### Phase 00 — Prérequis et spike

**Objectif** : lever l'inconnue technique majeure avant que quoi que ce soit soit construit dessus.

**Contexte**
Tout l'éditeur de la phase 07 et la synchronisation de la phase 06 dépendent d'**horodatages
mot à mot**. whisper.cpp les produit par alignement DTW, pas par les horodatages de segment
ordinaires. Si `whisper-rs` n'expose pas le DTW de façon exploitable, le repli est une
synchronisation **au segment** — fonctionnelle mais nettement moins agréable, et qui change
la conception des phases 06 et 07. Il faut le savoir maintenant.

**État constaté de la machine le 13/08/2026** :
| Outil | Version | Verdict |
|---|---|---|
| Rust / Cargo | 1.90.0 (`x86_64-pc-windows-msvc`) | OK |
| Visual Studio | 2022 Community | OK |
| CMake | 4.1.0 | OK |
| ffmpeg | 7.1 (PATH) | OK, mais l'app embarquera son propre binaire |
| Ollama | 0.32.9 | OK |
| Pilote NVIDIA | 580.97 | OK (supporte CUDA 13) |
| **CUDA Toolkit** | **11.0** | **BLOQUANT** |
| **libclang** | **absent** | **BLOQUANT** |

**Le toolkit doit satisfaire deux contraintes indépendantes** — les deux ont été
rencontrées sur cette machine :

1. **Cibler `sm_89`** (Ada). CUDA 11.0 ne le peut pas :
   `nvcc fatal : Value 'sm_89' is not defined for option 'gpu-architecture'`.
   Acquis depuis 11.8. Le dossier `v11.8` de la machine est **vide** : ce n'est pas un toolkit.
2. **Accepter le MSVC installé.** `crt/host_config.h` refuse tout compilateur hors de sa
   fenêtre. **CUDA 12.0 rejette `_MSC_VER >= 1940`** alors que la machine porte MSVC 14.41
   (1941) et 14.44 (1944) : le build meurt en 3 s sur `CMakeCUDACompilerId.cu`, sans avoir
   rien compilé. Un toolkit trop récent pour l'architecture **et** trop ancien pour le
   compilateur échoue de deux façons différentes.

→ **Exiger CUDA 12.9 ou 13.x** (support de `_MSC_VER` 1944). Contrôle avant installation :
`grep _MSC_VER "<CUDA>/include/crt/host_config.h"`.

Installer les composants **sans le pilote** (celui de la machine est plus récent), puis
faire pointer `CUDA_PATH` dessus — `build.rs` le lit directement pour trouver `lib/x64`.
Épingler l'architecture avec `CUDAARCHS=89` : `build.rs` ne définit pas
`CMAKE_CUDA_ARCHITECTURES`, et sans cela ggml compile pour toutes les architectures connues.

**libclang est obligatoire** : bindgen en a besoin, et `WHISPER_DONT_GENERATE_BINDINGS`
n'est pas une échappatoire sur Windows (le `bindings.rs` fourni est généré sous Linux et
casse les assertions de layout MSVC). Le spike l'obtient via `spike/.venv-libclang`.

**Résultat du spike — voir `spike/RESULTS.md`** :
- **GO** sur les horodatages mot à mot. `DtwModelPreset::LargeV3Turbo` existe, couverture
  DTW **100 %**, monotone, dans les bornes. Le repli au segment est écarté.
- **Conflit découvert** : whisper.cpp 1.8.3 neutralise le callback de streaming dès que le
  DTW est actif. Mesuré : DTW activé → **0** callback ; DTW désactivé → 1 callback mais
  **0 %** de couverture DTW. Les deux fonctionnalités s'excluaient.
- **Décision de l'utilisateur** : vendoriser un fork corrigé plutôt que découper l'audio
  nous-mêmes. Voir [vendor/PATCHES.md](vendor/PATCHES.md), PATCH 001. Après correctif :
  streaming **et** couverture DTW 100 % simultanément.
- `t_dtw` est un **point d'ancrage par token**, pas un intervalle. Les intervalles de mots
  se dérivent (un mot finit là où le suivant commence). Conséquence directe pour la phase 06.

**Tâches**
1. Installer CUDA Toolkit ≥ 12.4 ; vérifier `nvcc --version` et une compilation triviale `sm_89`.
2. Créer le dépôt local, `.gitignore`, pousser les quatre documents de référence.
3. Binaire Rust jetable (`spike/`) : charger `ggml-large-v3-turbo.bin`, transcrire un vrai
   fichier français, activer `token_timestamps` **et** le DTW avec les *alignment heads* de
   large-v3-turbo, écrire un JSON `{ segments: [{ start, end, text, words: [{ text, start, end, prob }] }] }`.
4. Mesurer : durée de traitement pour ~15 min d'audio, VRAM occupée, précision apparente des
   horodatages mot à mot sur 20 mots vérifiés à l'oreille.
5. Vérifier le VAD Silero de whisper.cpp sur un fichier comportant de longs silences.
6. Application Tauri minimale qui embarque ce build CUDA, **installée puis lancée hors de
   l'arbre de développement**, pour prouver que les DLL cuBLAS/cudart suivent.

**Critères d'acceptation**
- [x] Le JSON contient des horodatages **par mot**, monotones, dans les bornes du segment
      — couverture DTW 100 % sur `fixtures/speech-fr.wav`
- [x] Les intervalles de mots sont non dégénérés (aucun `end_ms <= start_ms`)
- [x] **PATCH 001** : streaming **et** couverture DTW 100 % sur un fichier mono-bloc
- [x] **PATCH 001 sur un fichier multi-bloc** (> 30 s) — 8 callbacks sur 3 blocs,
      couverture DTW 100 %
- [x] Alignement vérifié objectivement — **28/28 mots réels dans la parole, 0 dans le
      silence**, précision ≈ 10 ms (seuil visé : 200 ms)
- [x] Build CUDA réussi — CUDA 12.9.41, `sm_89`, binaire 48,5 Mo (contre 3 Mo en CPU)
- [x] Temps et VRAM consignés — **12,1× temps réel**, **2 353 Mo** de VRAM, sur 12 min 06 s
      de français réel ; 2 815 mots, couverture DTW 100 %, 612 callbacks de streaming
- [x] **Silero vérifié** : détection correcte des plages de parole (16,58–24,83 s et
      42,40–50,62 s contre 15,00–25,79 s et 40,79–51,58 s réelles). L'intégration passe
      en phase 04, côté Rust — le chemin `FullParams` est inutilisable (PATCH 002 annulé)
- [x] Exécution hors arbre de dev, `PATH` réduit à `System32` — **code 0, backend CUDA**,
      après ajout des 3 DLL CUDA et de ffmpeg. Charge utile **923 Mo** (dont 637 Mo pour
      `cublasLt64_12.dll` seule), plus 1,62 Go de modèle téléchargé au premier lancement.
      Piste de réduction pour la phase 12 : `nvprune` vers `sm_89` uniquement
- [x] **Décision GO / REPLI remontée à l'utilisateur** — GO, repli au segment écarté

> **Point d'arrêt.** Si le DTW est inexploitable, ne pas continuer seul : présenter le repli
> au segment à l'utilisateur et attendre sa décision. C'est la seule phase qui bloque.

**Commit** : `phase 00: cuda prerequisites and whisper-rs dtw spike`

---

### Phase 01 — Coque Tauri et système de thème

**Objectif** : la fenêtre, les jetons, l'accent vivant. Aucune fonctionnalité métier.

**Contexte**
L'accent est une variable transverse : il re-teinte le fond maillé, les bordures pointillées,
les barres de forme d'onde, le vumètre et la couleur du texte posé sur l'accent. Le construire
correctement maintenant évite de le rétro-ajuster dans douze composants plus tard.

**Tâches**
1. Projet Tauri v2 + frontend. Plugins : `dialog`, `fs`, `notification`, `single-instance`.
2. Fenêtre 1360×900, `decorations: false`, redimensionnable, barre de titre custom 46 px
   (DESIGN.md §6) avec `data-tauri-drag-region` et les trois contrôles fonctionnels.
3. Polices Figtree, Newsreader, JetBrains Mono embarquées en woff2 locaux. **Aucune requête
   vers `fonts.googleapis.com`** — à vérifier dans l'onglet réseau.
4. `styles/tokens.css` : les deux thèmes de DESIGN.md §2, au jeton près.
5. `lib/accent.ts` : `mix`, `lum`, `hsl2rgb`, dérivation de `--accentSoft`, `--dashed`,
   `--onAccent`, et génération du `--mesh` des deux thèmes.
6. Fond maillé en couche absolue `pointer-events: none` sur chaque écran.
7. Persistance du thème et de l'accent (fichier de configuration), restitution au démarrage.
8. Page de démonstration temporaire affichant tous les jetons, pour la vérification visuelle.

**Critères d'acceptation**
- [x] Chaque jeton de DESIGN.md §2.1 et §2.2 existe et vaut exactement la valeur indiquée
      — 36 comparaisons automatiques contre la table du prototype, plus les 22 valeurs
      calculées relevées dans la page
- [x] Basculer le thème ne recharge pas la page et ne provoque aucun flash — prouvé par une
      sentinelle conservée à travers thème, accents et saisie invalide
- [x] Changer l'accent re-teinte le maillage **et** `--onAccent` bascule correctement au
      passage du seuil 0.62 ~~(vérifier avec `#8E9A5B` puis `#E08A4B`)~~
      > **Correction du critère.** Ces deux accents n'encadrent pas le seuil : `lum` vaut
      > 0,5617 et 0,6139, tous deux **sous** 0,62, comme les cinq préréglages du §2.5. Le
      > basculement est vérifié avec `#D8D83C` (0,7773), atteignable à la roue. Détail et
      > décision restante : PROGRESS.md, phase 01.
- [x] `mix`, `lum`, `hsl2rgb` couverts par des tests unitaires, comparés aux valeurs du prototype
      — les fonctions sont **extraites de `Sillage.dc.html`** à l'exécution des tests
- [x] Aucune requête réseau au démarrage (onglet réseau vide) — 26 requêtes, 0 externe ;
      bundle de production audité, aucune référence à Google Fonts
- [ ] La fenêtre se déplace par la barre custom ; réduire / agrandir / fermer fonctionnent
      — contrôles couverts par des tests de composant, zone de drag posée et absente des
      boutons, fenêtre redimensionnable (`WS_THICKFRAME`) ; **le geste de déplacement à la
      souris reste à confirmer**
- [x] Thème et accent survivent au redémarrage — testé sur le magasin *et* sur l'état de
      commande, une seconde instance relisant le fichier

**Commit** : `phase 01: tauri shell, theme tokens, live accent system`

---

### Phase 02 — Stockage

**Objectif** : le dossier bibliothèque et la base, sans interface.

**Contexte**
Le modèle de données de CONCEPTION.md §3.4 est le contrat de toutes les phases suivantes.
`transcript_hash` porte à lui seul le mécanisme d'obsolescence de la phase 08 : il doit être
calculé sur le **texte affiché** (verbatim + corrections), pas sur le verbatim seul.

**Tâches**
1. Dossier bibliothèque configurable, défaut `%USERPROFILE%\Documents\Sillage`,
   arborescence de CONCEPTION.md §3.4.
2. SQLite (`rusqlite`) + migrations versionnées. Tables : `transcripts`, `segments`, `words`,
   `tags`, `transcript_tags`, `llm_outputs`, `queue_items`, `settings`.
3. Table virtuelle FTS5 sur titre + verbatim + résumé, avec déclencheurs de synchronisation.
4. Sérialisation JSON des transcriptions, format binaire compact des pics.
5. Fonctions de hachage : SHA-256 du média, `transcript_hash` du texte affiché.
6. Migration du dossier bibliothèque (déplacement avec les données).

**Critères d'acceptation**
- [ ] Les migrations s'appliquent sur base vide **et** sur base existante, deux fois de suite
      sans erreur
- [ ] Une recherche FTS5 sur un accent français (`résumé`, `déjà`) retourne le bon résultat
- [ ] Un aller-retour transcription → JSON → transcription est exactement identique
- [ ] `transcript_hash` change quand une correction change, ne change pas quand un tag change
- [ ] Le déplacement du dossier bibliothèque conserve toutes les entrées et l'audio
- [ ] Tests d'intégration sur un dossier temporaire, nettoyé après

**Commit** : `phase 02: library folder, sqlite schema, fts5 search`

---

### Phase 03 — Ingestion

**Objectif** : transformer n'importe quel fichier accepté en PCM prêt pour Whisper, plus les pics.

**Contexte**
Le décodage produit du PCM 16 kHz mono f32. **Les pics de forme d'onde se calculent pendant
ce décodage**, à partir du PCM déjà en mémoire — jamais par un second décodage côté frontend.
La forme d'onde du lecteur (96 barres, DESIGN.md §8) sera un sous-échantillonnage de ces pics.

**Tâches**
1. Embarquer `ffmpeg.exe` en ressource Tauri ; ne jamais dépendre du PATH.
2. Détection de format et de piste : audio (m4a, mp3, wav, flac, ogg, opus, aac, wma) et
   vidéo (mp4, mov, mkv, avi, webm) avec extraction de la piste audio.
3. Décodage → PCM 16 kHz mono f32, en flux, sans charger l'intégralité en mémoire.
4. Calcul des pics pendant le décodage, résolution suffisante pour un rendu à 96 barres
   **et** pour un zoom ultérieur.
5. SHA-256 en flux ; détection de doublon.
6. Copie dans `media/` sous l'identifiant interne, extension d'origine conservée.
7. Taxonomie d'erreurs de CONCEPTION.md §8, chacune avec un message français actionnable.
8. Attente de stabilisation de taille pour les fichiers en cours d'écriture.

**Critères d'acceptation**
- [ ] Un fichier de chaque format listé s'ingère (jeu d'essai versionné ou généré par ffmpeg)
- [ ] Un `.mp4` avec piste audio s'ingère ; un `.mp4` muet est **rejeté** avec le message dédié
- [ ] Fichier corrompu, durée nulle, durée < 1 s : rejetés, messages distincts, en français
- [ ] Un doublon est détecté par SHA-256 et propose les deux issues (§8)
- [ ] Les pics d'un fichier de 15 min pèsent moins de 200 Ko
- [ ] Un fichier de 2 h s'ingère sans dépasser 500 Mo de RSS
- [ ] Aucun appel à un `ffmpeg` du PATH (vérifier en renommant le ffmpeg système)

**Commit** : `phase 03: ffmpeg ingestion, format support, waveform peaks, dedup`

---

### Phase 04 — Moteur de transcription et file

**Objectif** : le cœur. Transcription en streaming, file séquentielle, gestion des modèles.

**Contexte**
CONCEPTION.md §3.3 : **strictement séquentiel**, Whisper turbo reste résident (~2 Go).
Le streaming de la phase 05 dépend du `new_segment_callback` de whisper.cpp relayé en
événements Tauri. La file doit survivre à une fermeture : elle est persistée en base, pas
en mémoire.

**Tâches**
1. Intégration `whisper-rs` avec CUDA, DTW et VAD Silero, selon les conclusions de la phase 00.
   **Le VAD se fait côté Rust, pas via `FullParams`.** Décision prise en phase 00 après
   l'échec de PATCH 002 : le drapeau `vad` de `FullParams` est **silencieusement ignoré**
   par le chemin `whisper_full_with_state` qu'utilise whisper-rs, et le déplacer provoque
   un segfault (`ctx->state` est nul). Utiliser l'API VAD exportée par whisper-rs
   (`whisper_vad::*`) : détecter les plages de parole, transcrire chacune avec un décalage
   connu, additionner les décalages aux horodatages DTW. Silero est fiable — mesuré à
   ±1,6 s sur les bornes de parole de `fixtures/vad-test.wav`. Détail : vendor/PATCHES.md.
   Prévoir un test de non-régression comparant le nombre de segments avec et sans VAD.
2. Gestion des modèles : téléchargement avec progression (octets, débit, ETA), vérification
   d'intégrité, `large-v3-turbo` et `large-v3`, suppression, emplacement configurable.
   **Afficher la taille réelle du fichier, pas celle du prototype.**
3. Injection du vocabulaire personnalisé en `initial_prompt`.
4. Langue : auto-détection, ou forçage `fr` / `en` selon les réglages, surchargeable par fichier.
5. `new_segment_callback` → événements Tauri, débit maîtrisé (pas un événement par token).
   **Suppose PATCH 001** (vendor/PATCHES.md) : sans lui, ce callback ne part jamais quand
   le DTW est actif. Persister les bornes de segment **dérivées des mots**, pas celles
   renvoyées par whisper — voir spike/RESULTS.md §4.2.
6. File séquentielle persistée : positions, reprise au lancement, une seule tâche active.
7. Progression et ETA dérivés de la **position audio réelle**.
8. Gestion de l'échec de chargement pour VRAM insuffisante, message de CONCEPTION.md §8.
9. Re-transcription d'une entrée existante avec un autre modèle ou une autre langue.

**Critères d'acceptation**
- [ ] Un `.m4a` français de 15 min produit une transcription complète avec mots horodatés
- [ ] Les segments arrivent au frontend **au fil de l'eau**, pas d'un bloc à la fin
- [ ] La progression est monotone et l'ETA converge ; aucune barre décorative
- [ ] Fermer l'app pendant un traitement puis la rouvrir **reprend la file** au bon endroit
- [ ] Cinq fichiers déposés ensemble sont traités **un par un**, jamais en parallèle
- [ ] Un modèle absent déclenche le téléchargement, la file attend, l'app reste utilisable
- [ ] Le téléchargement s'annule proprement et se reprend
- [ ] Le vocabulaire personnalisé change la sortie sur un fichier contenant un nom propre visé
- [ ] Forcer `en` sur un fichier français produit bien de l'anglais (preuve que le forçage agit)
- [ ] Une VRAM insuffisante produit le message dédié, **pas un crash**

**Commit** : `phase 04: whisper engine, streaming transcription, persisted sequential queue`

---

### Phase 05 — Écran Bibliothèque

**Objectif** : l'écran 01 du design, pixel-perfect, branché sur le vrai moteur.

**Référence** : DESIGN.md §7. **Lire la section entière avant de commencer.**

**Tâches**
1. Grille 232 px + 1fr, barre latérale complète : navigation avec compteurs réels,
   liste de tags, carte « Moteur » avec l'état réel du modèle Whisper et d'Ollama.
2. Barre de recherche branchée sur FTS5, résultats en direct, raccourci `Ctrl+Shift+F`.
3. Bouton « Enregistrer » (ouvre la modale de la phase 09 — inerte ici).
4. Zone de dépôt : glisser-déposer **sur toute la fenêtre**, sélecteur de fichiers,
   champ URL (inerte ici).
5. Cartes dans tous leurs états : en cours avec streaming et curseur clignotant, normale,
   en échec, ligne d'attente.
6. Titre éditable au clic (affordance « modifier »).
7. Filtrage par tag, états « aucun résultat » et « filtre actif » de DESIGN.md §11.

**Critères d'acceptation**
- [ ] Chaque métrique de DESIGN.md §7 vérifiée dans l'inspecteur — sidebar 232 px, barre de
      recherche 40 px, progression 4 px, rayons 11/12/14/16 px, `gap` 4/6/10/12/14/16/22 px
- [ ] Le texte en streaming s'affiche en **Newsreader 15px/1.6**, `--dim` pour l'établi,
      `--text` pour les derniers mots, curseur accent 8×15 px
- [ ] Les métadonnées des cartes normales sont en `--faint`, celles de la carte en cours en `--dim`
- [ ] Déposer un fichier n'importe où dans la fenêtre fonctionne, avec état de survol visible
- [ ] La recherche trouve un mot présent **uniquement dans le corps** d'une transcription
- [ ] Rendu correct dans les deux thèmes et avec au moins deux accents
- [ ] État vide conforme à DESIGN.md §11

**Commit** : `phase 05: library screen with live queue, search, tags and drop zone`

---

### Phase 06 — Écran Détail et lecteur

**Objectif** : l'écran 02, lecture audio synchronisée au texte.

**Référence** : DESIGN.md §8.

**Contexte**
C'est l'écran qui justifie tout le reste. Le repère de lecture principal est l'**atténuation
des segments non courants** : le segment lu est en `--text`, les autres en `--dim`, son
horodatage passe en `--accent`. Le mot lu porte le surlignage `--accentSoft` avec sa règle
intérieure. Sans cela, l'écran est un mur de texte.

**Deux règles issues du spike (phase 00), non négociables :**
1. `t_dtw` est un **instant d'ancrage par token**, pas une durée. Les intervalles de mots
   se dérivent : un mot finit là où le suivant commence, le dernier avec son segment.
   Sans cette dérivation, tout mot d'un seul token a `start == end` et le surlignage
   ne peut pas fonctionner.
2. **Ne jamais afficher les horodatages de segment de whisper.** Ils sont faux après un
   silence — écart mesuré jusqu'à **16,6 s**. Les bornes d'un segment se dérivent de son
   premier et de son dernier mot. Vaut aussi pour la colonne d'horodatages de l'écran 02.

Détail et mesures : [spike/RESULTS.md](spike/RESULTS.md) §4.

**Tâches**
1. Colonne 880 px centrée, en-tête, métadonnées, tags, indicateur « Édité ».
2. Lecteur : 96 barres issues des pics de la phase 03, barres lues en `--accent`, temps,
   vitesse, volume.
3. Lecture, pause, déplacement au clic sur la forme d'onde.
4. Surlignage mot à mot synchronisé, défilement automatique **désactivé dès que l'utilisateur
   défile à la main**, réactivable.
5. Clic sur un mot → lecture depuis ce mot.
6. Bascules « horodatages » et « confiance » (état actif = bordure et texte en `--accent`).
7. Affichage de la faible confiance : `border-bottom: 2px dotted var(--warn)`.
8. Raccourcis de CONCEPTION.md §6, actifs sans quitter le curseur texte.
9. Panneaux LLM en coquilles statiques dans leurs quatre états (remplis en phase 08).

**Critères d'acceptation**
- [ ] Métriques de DESIGN.md §8 vérifiées — colonne 880 px, transcription Newsreader
      19.5px/1.78, `gap` 20 px, horodatage `min-width` 52 px, lecteur 46 px de haut, 96 barres
- [ ] Le segment courant est en `--text` et son horodatage en `--accent`, les autres en
      `--dim` / `--faint`
- [ ] Le mot lu est surligné et le surlignage **suit l'audio** avec une dérive perceptible nulle
- [ ] Cliquer un mot démarre la lecture à cet endroit (± 150 ms)
- [ ] Défiler à la main désactive le suivi automatique ; un contrôle le réactive
- [ ] `Ctrl+Shift+Space`, `Ctrl+←/→`, `Ctrl+↑/↓` fonctionnent **pendant** la saisie de texte
- [ ] `Ctrl+Space` n'est câblé nulle part (réservé à Push-to-talk)
- [ ] Une transcription de 2 h reste fluide au défilement (virtualisation si nécessaire)

**Commit** : `phase 06: transcript detail screen, waveform player, word-level sync`

---

### Phase 07 — Éditeur

**Objectif** : corriger le texte sans jamais casser la synchronisation.

**Contexte** : CONCEPTION.md §3.5. Le verbatim est **immuable**. Les corrections sont une couche
au-dessus, indexée par identifiants de mots stables. Un mot inséré hérite de l'intervalle
temporel de ses voisins par interpolation.

**Tâches**
1. Édition en ligne dans le flux, **sans mode « édition » séparé**.
2. Couche de corrections : remplacement, insertion, suppression, par identifiant de mot.
3. Interpolation temporelle des mots insérés.
4. Recalcul de `transcript_hash` à chaque correction stabilisée.
5. Indicateur « Édité · revenir au verbatim » et restauration intégrale.
6. Marqueur « corrigé » (DESIGN.md §8) sur les segments modifiés.
7. Annuler / rétablir sur la couche de corrections.
8. `Ctrl+F` : recherche dans la transcription, avec remplacement (noms propres récurrents).

**Critères d'acceptation**
- [ ] Corriger un mot conserve la synchronisation de tous les autres
- [ ] Insérer trois mots au milieu d'un segment garde le clic-pour-lire fonctionnel
- [ ] Supprimer un mot ne décale aucun horodatage voisin
- [ ] « Revenir au verbatim » restaure exactement l'état d'origine, y compris les segments
      marqués « corrigé »
- [ ] Le verbatim d'origine reste intact en base après cinquante corrections
- [ ] `transcript_hash` change à chaque correction stabilisée, pas à chaque frappe
- [ ] Annuler / rétablir traverse correctement les trois types d'opérations
- [ ] Les corrections survivent au redémarrage

**Commit** : `phase 07: corrections layer with stable word ids and verbatim restore`

---

### Phase 08 — Couche LLM

**Objectif** : les quatre sorties, deux fournisseurs, l'obsolescence.

**Référence** : DESIGN.md §8 pour les quatre états de panneau.

**Contexte**
Chaînage **automatique par défaut**, séquentiel, après la transcription et **jamais pendant**
(CONCEPTION.md §3.3). Ollama par défaut sur `localhost:11434`.

**Tâches**
1. Abstraction de fournisseur : Ollama et cloud (clé lue dans les réglages, jamais versionnée).
2. Quatre types de sortie : version nettoyée, résumé, notes structurées, prompts personnalisés.
3. Prompts en français, versionnés (`prompt_version`), éditables pour les prompts personnalisés.
4. Gestion des prompts personnalisés : création, édition, suppression, réordonnancement.
5. Obsolescence : comparaison `source_transcript_hash` ↔ `transcript_hash`, badge `OBSOLÈTE`,
   bouton « Régénérer ». Le contenu périmé **reste affiché**.
6. Chaînage automatique après transcription, en respectant les réglages, **strictement séquentiel**.
7. Titre généré automatiquement (court, sans guillemets), éditable et marqué `title_is_custom`
   dès que l'utilisateur y touche — la régénération ne l'écrase alors plus.
8. Erreurs : Ollama injoignable, modèle absent, clé invalide, dépassement de contexte —
   chacune avec son message français actionnable et un « Réessayer ».
9. Découpage des transcriptions dépassant la fenêtre de contexte du modèle.
10. Détection de la taille du modèle Ollama au démarrage, avertissement au-delà de 12 Go
    (DESIGN.md §9, carte « Coexistence VRAM »).

**Critères d'acceptation**
- [ ] Les quatre types produisent un résultat en français sur une transcription française
- [ ] Une sortie désactivée dans les réglages apparaît **repliée avec « Générer »**, jamais absente
- [ ] Corriger le texte marque les sorties `OBSOLÈTE` sans effacer leur contenu
- [ ] « Régénérer » lève le badge et mémorise le nouveau hash
- [ ] Le pied de panneau affiche fournisseur, modèle et date réels
- [ ] Ollama éteint → message dédié, transcription intacte, « Réessayer » fonctionnel
- [ ] Le chaînage automatique respecte les réglages et ne chevauche **jamais** une transcription
- [ ] Un titre modifié à la main n'est plus jamais écrasé
- [ ] Une transcription de 2 h passe le résumé sans dépassement de contexte
- [ ] Un modèle Ollama > 12 Go déclenche l'avertissement VRAM

**Commit** : `phase 08: llm provider layer, four output kinds, staleness tracking`

---

### Phase 09 — Points d'entrée

**Objectif** : les trois entrées supplémentaires retenues.

**Référence** : DESIGN.md §10 pour les deux modales.

**Tâches**
1. Menu contextuel Explorer : « Transcrire avec Sillage » sur toutes les extensions supportées,
   clés `HKCU`, posées par l'installeur, activables et désactivables depuis les réglages.
2. Instance unique : un second lancement route le fichier vers l'app ouverte et la met au premier plan.
3. Modale d'enregistrement : sélection du micro, vumètre 44 barres, chrono, pause,
   « Arrêter et transcrire » → entrée en file.
4. Import URL : collage, résolution par `yt-dlp` (embarqué), aperçu titre / durée / poids /
   domaine **avant** téléchargement, confirmation, mise en file.
5. Collage d'un chemin de fichier local dans le même champ.
6. Notification système en fin de traitement **uniquement si la fenêtre n'a pas le focus**.

**Critères d'acceptation**
- [ ] Clic droit sur un `.m4a` dans l'Explorateur → l'app s'ouvre avec le fichier en traitement
- [ ] Avec l'app déjà ouverte, le clic droit **n'ouvre pas** de seconde fenêtre
- [ ] La désinscription depuis les réglages retire réellement l'entrée du menu contextuel
- [ ] L'enregistrement produit un fichier lisible qui s'ingère normalement
- [ ] Le vumètre réagit au son réel du micro
- [ ] Pause puis reprise produit un enregistrement continu, sans coupure
- [ ] Une URL affiche l'aperçu **avant** tout téléchargement, et « Annuler » ne télécharge rien
- [ ] Notification en fin de traitement fenêtre non focalisée ; **aucune** fenêtre focalisée

**Commit** : `phase 09: explorer integration, recording modal, url import`

---

### Phase 10 — Réglages

**Objectif** : les cinq onglets, dont l'onglet **Apparence** demandé par l'utilisateur.

**Référence** : DESIGN.md §9 pour le châssis, **§12 pour l'onglet Apparence**.

**Contexte**
L'onglet Apparence n'existe pas dans le prototype : ses composants sont repris à l'identique
du spécimen d'en-tête (roue, pastille, hex, préréglages, segmenté de thème). Ne rien inventer,
tout est spécifié dans DESIGN.md §12.

**Tâches**
1. Châssis : titre, onglets en pastilles, lignes libellé / indice / valeur.
2. Onglet **Transcription** : modèle, langue, VAD, horodatages, vocabulaire personnalisé.
3. Onglet **Post-traitement** : mode, fournisseur, modèle, clé API, quatre sorties activables,
   gestion des prompts personnalisés, carte « Coexistence VRAM ».
4. Onglet **Bibliothèque** : emplacement du dossier avec migration, conservation de l'audio,
   dossier surveillé (présent, désactivé par défaut).
5. Onglet **Système** : menu contextuel Explorer, lancement au démarrage, modèles installés
   avec tailles réelles, téléchargement et suppression.
6. Onglet **Apparence** : roue 320/160, pastille + hex éditable, 5 préréglages, segmenté
   Sombre / Clair — DESIGN.md §12 au pixel près.
7. Application immédiate de tout changement, sans bouton de validation.

**Critères d'acceptation**
- [ ] Les cinq onglets existent et respectent les métriques de DESIGN.md §9
- [ ] La roue reproduit exactement `hsl(h, 0.55·s + 0.12, 0.60 − 0.06·s)` — vérifier trois
      points connus au pixel près
- [ ] Cliquer la roue re-teinte l'application entière **immédiatement**, maillage compris
- [ ] Le canvas est en 320×320 internes pour 160×160 affichés (net en HiDPI)
- [ ] Les 5 préréglages sont exactement ceux de DESIGN.md §2.5, dans l'ordre
- [ ] Le hex est saisissable, validé, normalisé en majuscules ; une saisie invalide est refusée
      sans casser l'accent courant
- [ ] `--onAccent` bascule correctement de part et d'autre du seuil 0.62
- [ ] Tout changement est persisté immédiatement et survit au redémarrage
- [ ] Aucun réglage ne rend une fonctionnalité inaccessible depuis l'écran de détail (règle B.4)

**Commit** : `phase 10: settings screens including appearance tab with accent wheel`

---

### Phase 11 — Exports

**Objectif** : sortir le travail de l'application.

**Référence** : CONCEPTION.md §7.

**Tâches**
1. Formats : `.txt`, `.md`, `.docx`, `.srt`, `.vtt`, `.json`, presse-papiers.
2. Option « inclure les sorties LLM ».
3. Option verbatim d'origine **ou** texte corrigé (corrigé par défaut).
4. Option horodatages pour `.txt` et `.md`.
5. Dialogue d'export cohérent avec le langage visuel des modales (DESIGN.md §10).
6. Nom de fichier par défaut dérivé du titre, assaini pour Windows.

**Critères d'acceptation**
- [ ] Les six formats s'ouvrent sans erreur dans un lecteur adapté
- [ ] Le `.srt` se charge dans VLC avec des sous-titres correctement calés
- [ ] Le `.json` contient la structure complète, mots et probabilités compris
- [ ] Les exports reflètent les corrections par défaut ; l'option verbatim donne l'origine
- [ ] Les accents français survivent à tous les formats (UTF-8, `.srt` compris)
- [ ] Un titre contenant `: / \ ? *` produit un nom de fichier valide

**Commit** : `phase 11: exports in six formats with llm output inclusion`

---

### Phase 12 — Finition et installeur

**Objectif** : combler tout ce qui reste et produire un installeur qui fonctionne.

**Référence** : DESIGN.md §11 et §13, CONCEPTION.md §8.

**Tâches**
1. Écran de premier lancement (DESIGN.md §11), avec les **tailles réelles** de modèle.
2. Tous les états vides et d'erreur de DESIGN.md §11.
3. Passe complète sur les cas limites de CONCEPTION.md §8.
4. Passe complète sur l'inventaire d'états de DESIGN.md §13, **dans les deux thèmes**.
5. Raccourcis complets et écran d'aide les listant.
6. Installeur MSI/NSIS : DLL CUDA, ffmpeg, yt-dlp, polices, clés de registre du menu contextuel.
7. Désinstallation propre : registre nettoyé, **dossier bibliothèque préservé**.
8. Journalisation dans un fichier, avec un moyen de l'ouvrir depuis les réglages.
9. Icône d'application et de fenêtre.
10. `README.md` : installation, prérequis, dépannage.

**Critères d'acceptation**
- [ ] Tous les cas de CONCEPTION.md §8 vérifiés un par un, avec le message français attendu
- [ ] Tous les états de DESIGN.md §13 vérifiés dans les deux thèmes
- [ ] L'installeur fonctionne sur une session Windows propre, sans Rust, Node ni CUDA installés
- [ ] Premier lancement : téléchargement du modèle, dépôt possible pendant le téléchargement
- [ ] La désinstallation retire les clés de registre et **conserve** la bibliothèque
- [ ] Aucune requête réseau hors téléchargement de modèle, résolution d'URL et LLM cloud explicite
- [ ] Un cycle complet fonctionne de bout en bout : clic droit Explorer → transcription →
      correction → régénération du résumé → export `.docx`
- [ ] `cargo clippy` et le lint frontend sont propres, sans `allow` ajouté pour l'occasion

**Commit** : `phase 12: first-run, empty and error states, installer, polish`
**Tag supplémentaire** : `v1.0.0`

---

## F. Ce qu'il ne faut pas faire

- Ne pas ajouter la diarisation. Écartée au round 1, hors périmètre.
- Ne pas ajouter de comptes, de synchronisation, de cloud pour la bibliothèque.
- Ne pas exposer `--blur` à l'utilisateur (DESIGN.md §4).
- Ne pas remplacer whisper.cpp par faster-whisper sans décision explicite de l'utilisateur.
- Ne pas charger de polices ou d'actifs depuis le réseau.
- Ne pas ajouter de télémétrie, de rapport d'erreur distant ou de vérification de mise à jour
  sans demande explicite.
- Ne pas « améliorer » les métriques du design en les trouvant trop serrées : les signaler.
- Ne pas remplacer le contenu d'exemple du prototype par de la donnée réelle **dans le
  prototype** : il est en lecture seule.

---

## G. Suivi

Tenir un `PROGRESS.md` à la racine, mis à jour à chaque fin de phase :

```markdown
## Phase NN — <titre>
- Statut : terminée / en cours / bloquée
- Commit : <sha>  ·  Tag : phase-NN
- Vérifié le : <date>
- Écarts au design : <aucun | liste>
- Décisions prises en autonomie : <aucune | liste>
- Dette laissée : <aucune | liste>
```

Les rubriques « écarts », « décisions » et « dette » sont **obligatoires** : elles sont
la seule trace de ce qui a été tranché sans l'utilisateur.

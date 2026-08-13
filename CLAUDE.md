# Sillage

Application desktop Windows de transcription audio locale : dépôt de fichiers audio/vidéo →
transcription Whisper en streaming → éditeur synchronisé à l'audio → post-traitement LLM.

**Utilisateur unique, poste unique** (RTX 4090 Laptop, 16 Go VRAM). Pas de comptes, pas de cloud
pour la bibliothèque, pas de télémétrie.

---

## À lire avant de coder

| Document | Contenu |
|---|---|
| [ROADMAP.md](ROADMAP.md) | **Commencer ici.** Phases, critères d'acceptation, protocole git |
| [CONCEPTION.md](CONCEPTION.md) | Décisions produit et architecture — le *pourquoi* |
| [DESIGN.md](DESIGN.md) | Jetons, métriques, composants — le *à quoi ça ressemble* |
| `app-design-with-glassmorphism/project/Sillage.dc.html` | Prototype original, **lecture seule** |

Autorité en cas de contradiction : prototype > DESIGN.md > CONCEPTION.md > ROADMAP.md.

---

## Pile

Tauri v2 · Rust · whisper.cpp via `whisper-rs` (CUDA, DTW, VAD Silero) · SQLite + FTS5 ·
ffmpeg et yt-dlp embarqués · Ollama en local par défaut.

**Pas de Python.** Le sidecar Python a été explicitement écarté.

---

## Invariants

1. **Interface en français**, mot pour mot celles du prototype quand elles y figurent.
   Code, chemins et commits en anglais.
2. **Rien n'est destructif.** L'audio et la structure complète des segments sont toujours
   conservés. Le verbatim est immuable ; les corrections sont une couche au-dessus.
3. **Les réglages fixent des défauts, jamais des interdits.** Une sortie LLM désactivée
   apparaît repliée avec un bouton « Générer », jamais absente.
4. **Traitement strictement séquentiel.** Whisper et Ollama ne tournent jamais en même temps.
5. **Aucun accès réseau non sollicité.** Polices embarquées, modèles téléchargés sur action seule.
6. **Fidélité au design.** Aucune valeur inventée : si elle manque dans DESIGN.md, la lire dans
   le prototype et compléter DESIGN.md dans le même commit.
7. **`Ctrl+Space` est interdit** — réservé par le projet Push-to-talk de l'utilisateur.

---

## Git

Branche unique `main`, un tag `phase-NN` par phase. Committer **et pousser** à chaque fin de
phase vérifiée, jamais avant. Protocole complet : ROADMAP.md §C.

Ne jamais committer : `models/`, `library/`, `.env`, binaires ffmpeg/yt-dlp, artefacts de build.

---

## Projets voisins de l'utilisateur

- `D:\Documents\MANTARA\Push to talk` — dictée Windows, faster-whisper, PySide6.
  Utile pour : téléchargement de modèle avec progression, amorçage CUDA, capture micro.
  **Possède `Ctrl+Space` en global.**
- `D:\Documents\MANTARA\AI COMPAGNON APP` — contient `ggml-large-v3-turbo.bin` (1,62 Go)
  et un serveur faster-whisper de référence.

---

## Prérequis de build

Constatés en phase 00, sur la machine, le 13/08/2026. Aucun n'est optionnel.

1. **CUDA Toolkit compatible avec le MSVC installé.** Deux contraintes indépendantes :
   - **Architecture** : le toolkit doit cibler `sm_89` (Ada). CUDA 11.0 ne le peut pas
     (`nvcc fatal : Value 'sm_89' is not defined`) ; c'est acquis depuis 11.8.
   - **Compilateur hôte** : `crt/host_config.h` du toolkit refuse tout MSVC hors de sa
     fenêtre. **CUDA 12.0 rejette `_MSC_VER >= 1940`** ; or la machine porte MSVC 14.41
     (1941) et 14.44 (1944). Le build échoue en 3 s sur `CMakeCUDACompilerId.cu`.
   → **Exiger un toolkit supportant `_MSC_VER` 1944 : CUDA 12.9 ou 13.x.**
     Vérifier avant d'installer :
     `grep _MSC_VER "<CUDA>/include/crt/host_config.h"`.

   Le dossier `v11.8` présent sur la machine est **vide** : ce n'est pas un toolkit.
2. **`CUDA_PATH` doit pointer vers le toolkit 12.x.** `vendor/whisper-rs-sys/build.rs` le lit
   directement pour trouver `lib/x64` ; s'il pointe encore vers v11.0, le build CUDA vise le
   mauvais toolkit sans prévenir.
3. **`libclang.dll` obligatoire**, avec `LIBCLANG_PATH` positionné. bindgen en a besoin, et
   `WHISPER_DONT_GENERATE_BINDINGS` **n'est pas une échappatoire sur Windows** : le
   `bindings.rs` fourni par whisper-rs-sys 0.15.0 est généré sous Linux (types `_G_fpos_t`,
   `_IO_FILE`) et casse les assertions de layout MSVC. Le spike l'obtient via un venv
   contenu (`spike/.venv-libclang`, paquet PyPI `libclang`), pour ne pas imposer LLVM
   à l'échelle du système.

## Dépendance vendorisée

`vendor/whisper-rs-sys` est une copie patchée de whisper-rs-sys 0.15.0 (whisper.cpp 1.8.3),
branchée par `[patch.crates-io]`. Elle porte **PATCH 001**, sans lequel le streaming et les
horodatages mot à mot s'excluent mutuellement.

**Lire [vendor/PATCHES.md](vendor/PATCHES.md) avant toute montée de version de `whisper-rs`
ou `whisper-rs-sys`.** PATCH 001 est silencieux en cas de régression : le streaming cesse
simplement de fonctionner, sans erreur ni avertissement.

## Ingestion (phase 03)

`ffmpeg.exe` et `ffprobe.exe` sont **embarqués**, jamais pris sur le `PATH`. Ils ne sont pas
versionnés : lancer une fois

```powershell
powershell -ExecutionPolicy Bypass -File scripts/fetch-resources.ps1
```

qui les copie depuis le ffmpeg installé sur la machine (`-Download` pour les récupérer sur le
réseau, à la demande explicite seulement). Sans eux, les tests d'ingestion **s'ignorent en
affichant `IGNORÉ`** au lieu d'échouer — lire la sortie avant de conclure qu'une phase passe.

Quatre règles qui cassent en silence :

1. **Jamais `Command::new("ffmpeg")`.** Tout passe par `ingest::Tools::command`, qui tient deux
   chemins absolus. Un `ffmpeg` du `PATH` ferait passer les tests sur cette machine et
   échouerait chez l'utilisateur.
2. **`bundle.resources` de `tauri.conf.json` et `ingest::RESOURCE_DIR` doivent concorder.**
   Rien ne le vérifie à la compilation ; un désaccord se voit au premier fichier déposé.
3. **Le décodeur ne conserve jamais le PCM.** 2 h font 460 Mo, pour un budget de 500 Mo de RSS.
   Les pics se calculent au vol (`PeaksBuilder`) et les échantillons sont jetés. Mesuré :
   **8,8 Mo** de crête sur 2 h.
4. **La durée annoncée par le conteneur ne sert à rien refuser.** Seul le nombre d'échantillons
   sortis du décodeur fait foi ; un mp3 VBR sans en-tête Xing annonce n'importe quoi.
5. **ffmpeg n'a pas de séparateur `--`.** Le passer est accepté et ne protège de rien :
   l'argument suivant reste lu comme une option. Tout chemin remis à ffmpeg ou ffprobe doit être
   **absolu** (`Ingestor::ingest` s'en charge), sans quoi `-mémo.wav` revient en
   « Unrecognized option ».

## Stockage (phase 02)

Trois règles qui cassent **en silence** si on les enfreint. Détail : PROGRESS.md, phase 02.

1. **`data/<id>.json` fait foi ; la base est un index reconstructible.** Une transcription qui
   prend du retard sur son index se répare ; l'inverse est du travail perdu.
2. **Jamais `INSERT OR REPLACE` sur `transcripts`.** `REPLACE` supprime la ligne avant de la
   réinsérer, et `ON DELETE CASCADE` emporte sorties LLM, file et tags. Utiliser
   `INSERT … ON CONFLICT (id) DO UPDATE`, comme `save_transcript`.
3. **`body` et `transcript_hash` s'écrivent ensemble.** Les deux se dérivent du *texte affiché*
   (verbatim + corrections). N'en mettre qu'un à jour fait diverger la recherche du badge
   `OBSOLÈTE`, sans qu'aucun test générique ne s'en aperçoive.

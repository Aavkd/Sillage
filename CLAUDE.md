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

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

## Prérequis machine

CUDA Toolkit **≥ 12.4** obligatoire. La machine avait 11.0 au 13/08/2026, ce qui **ne peut pas**
cibler `sm_89` (Ada) — c'est la première tâche de la phase 00.

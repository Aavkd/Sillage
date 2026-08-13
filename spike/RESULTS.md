# Phase 00 — Résultats du spike

**Date** : 13 août 2026
**Question posée** : whisper-rs produit-il des horodatages mot à mot exploitables pour
l'éditeur synchronisé de Sillage ?

# VERDICT : GO

Le repli au niveau segment est **écarté**. Les phases 06 et 07 peuvent être construites
telles que conçues.

---

## 1. Ce qui est prouvé

| Point | Résultat |
|---|---|
| `DtwModelPreset::LargeV3Turbo` existe dans whisper-rs 0.16 | oui |
| Couverture DTW | **100 %** (13/13 mots, puis 32/32) |
| Horodatages monotones | oui |
| Intervalles de mots non dégénérés | oui, après dérivation |
| **Précision d'alignement** | **≈ 10 ms** sur un fichier de 66 s |
| Streaming + DTW simultanés | oui, **après PATCH 001** |
| Streaming multi-blocs | oui, 8 callbacks sur 3 blocs |

whisper-rs 0.16 expose par ailleurs tout ce dont la suite a besoin :
`set_initial_prompt` (vocabulaire personnalisé, phase 04), `set_vad_model_path` et
`set_vad_params` (VAD, phase 04), `set_progress_callback_safe` et `set_abort_callback_safe`
(progression et annulation, phases 04 et 05).

---

## 2. Mesure de précision de l'alignement

Faute de pouvoir écouter le fichier, la vérification est **objective** plutôt que
subjective : on construit un fichier dont les plages de parole sont connues au
milliseconde près, et on vérifie qu'aucun mot ne tombe ailleurs.

`fixtures/vad-test.wav` — 66,59 s : silence 15 s, parole 10,79 s, silence 15 s,
parole 10,79 s, silence 15 s. La parole occupe donc **15,00–25,79 s** et **40,79–51,58 s**.

| Contrôle | Résultat |
|---|---|
| Mots réels tombant dans le silence | **0** |
| Mots réels tombant dans la parole | **28 / 28** |
| Premier mot, bloc 1 | 16 860 ms — attendu 15 000 + 1 860 = **16 860 ms** |
| Premier mot, bloc 2 | 42 640 ms — attendu 40 790 + 1 850 = **42 640 ms** |

Le décalage de 1 860 ms correspond exactement au silence initial du clip source, mesuré
indépendamment lors du test mono-bloc (`'Ok,'` à 1 860 ms). **L'alignement est correct à
la dizaine de millisecondes près**, très en deçà du seuil de 200 ms fixé par la feuille
de route.

Quatre mots supplémentaires tombent dans le silence : ce sont des segments `'...'`
hallucinés. C'est précisément ce que le VAD doit supprimer (CONCEPTION §3.6).

---

## 3. PATCH 001 — le conflit streaming / DTW

**whisper.cpp 1.8.3 rend le streaming et les horodatages mot à mot mutuellement
exclusifs.** Les deux sites d'appel normaux du callback sont neutralisés dès que le DTW
est actif (`&& !ctx->params.dtw_token_timestamps`, lignes 7657 et 7702), au profit d'un
site spécifique au DTW défectueux à deux titres : il transmet un **index** là où tous les
autres transmettent un **compteur**, et sa borne de boucle compare un index à un compteur.

Mesures sur `fixtures/speech-fr.wav` (10,8 s) :

| | callbacks | couverture DTW |
|---|---|---|
| Avant correctif, DTW activé | **0** | 100 % |
| Avant correctif, DTW désactivé | 1 | **0 %** |
| Après correctif, DTW activé | **1** | **100 %** |

Sur `fixtures/vad-test.wav` (66,6 s, 3 blocs), après correctif : **8 callbacks**, aux
positions 0, 260, 19 980, 30 000, 40 000, 45 780, 50 520 et 60 000 ms. Le second défaut
— la borne de boucle, qui empêchait tout callback au-delà du premier bloc — est bien levé.

**Décision de l'utilisateur** : vendoriser un fork corrigé, plutôt que découper l'audio
nous-mêmes. Voir [../vendor/PATCHES.md](../vendor/PATCHES.md).

---

## 4. Conséquences de conception

### 4.1 `t_dtw` est un point, pas un intervalle

whisper fournit **un instant d'ancrage par token**, pas une durée. Les intervalles de mots
doivent être dérivés : un mot se termine là où le suivant commence, le dernier se terminant
avec son segment. Sans cette dérivation, tous les mots d'un seul token ont
`start_ms == end_ms` et le surlignage de l'éditeur ne peut pas fonctionner.

**Pour la phase 06.**

### 4.2 Ne jamais afficher les horodatages de segment de whisper

Ils sont **faux après un silence**. Mesuré sur `vad-test.wav` :

| Segment | whisper annonce | ses mots disent | écart |
|---|---|---|---|
| 1 | 260 ms | 16 860 ms | **16 600 ms** |
| 2 | 19 980 ms | 21 220 ms | 1 240 ms |
| 4 | 40 000 ms | 42 640 ms | 2 640 ms |
| 5 | 45 780 ms | 47 020 ms | 1 240 ms |

Les bornes affichées d'un segment doivent être **dérivées du premier et du dernier mot**,
jamais lues dans `whisper_full_get_segment_t0/t1`. Le spike le fait désormais et conserve
les valeurs brutes sous `raw_start_ms` / `raw_end_ms` pour mesurer l'écart.

**Pour les phases 04, 05 et 06.** La colonne d'horodatages de l'écran 02 et le texte en
streaming de l'écran 01 dépendent tous deux de ce point.

### 4.3 Le VAD n'est pas optionnel — mais il ne passe pas par `FullParams`

Sans VAD, 45 s de silence pur produisent **quatre segments hallucinés** `'...'`. Sur un
enregistrement réel comportant des pauses, cela pollue la transcription et la couche LLM.

Le drapeau `vad` de `FullParams` est pourtant **inutilisable** :

1. `set_vad_model_path()` seul ne suffit pas ; il faut aussi `enable_vad(true)`.
2. Même ainsi, whisper.cpp n'implémente le VAD que dans `whisper_full()`, alors que
   whisper-rs n'appelle que `whisper_full_with_state()`. Le drapeau est **accepté et
   silencieusement ignoré**.
3. Déplacer le bloc VAD (PATCH 002) provoque un **segfault systématique** : whisper-rs
   initialise le contexte en `_no_state`, donc `ctx->state` est nul, hypothèse que le
   chemin VAD amont ne fait jamais. **Correctif annulé.**

Silero lui-même est fiable : plages détectées 16,58–24,83 s et 42,40–50,62 s, contre
15,00–25,79 s et 40,79–51,58 s réelles.

**Décision** : faire le VAD côté Rust en phase 04, via `whisper_vad::*` qu'exporte
whisper-rs — détecter les plages de parole, transcrire chacune avec un décalage connu.

### 4.4 Le fork ne se recompilait pas — PATCH 003

Le piège le plus coûteux de la phase. `build.rs` ne surveillait que `wrapper.h`, **et** ne
copiait les sources dans `OUT_DIR` qu'à la création du dossier. Toute modification du C++
vendorisé donnait un `cargo build` réussi qui **réutilisait l'ancienne bibliothèque**.

PATCH 001 n'a fonctionné que parce qu'il était présent à la toute première compilation.
Contrôle obligatoire avant d'interpréter le moindre test :

```bash
grep -c "SILLAGE PATCH 001" spike/target/release/build/whisper-rs-sys-*/out/whisper.cpp/src/whisper.cpp
```

---

## 5. Prérequis de build découverts

1. **CUDA Toolkit ≥ 12.4.** Le seul toolkit complet de la machine est en **11.0**, qui
   rejette `sm_89` : `nvcc fatal : Value 'sm_89' is not defined for option
   'gpu-architecture'`. Le dossier `v11.8` est **vide** — ce n'est pas un toolkit.
   Installer les composants **sans le pilote** (celui de la machine est plus récent),
   puis faire pointer `CUDA_PATH` dessus : `build.rs` le lit directement.
2. **`libclang.dll` obligatoire.** `WHISPER_DONT_GENERATE_BINDINGS=1` **n'est pas une
   échappatoire sur Windows** : le `bindings.rs` fourni est généré sous Linux (types
   `_G_fpos_t`, `_IO_FILE`) et casse les assertions de layout MSVC. Le spike l'obtient
   via un venv contenu, pour ne pas imposer LLVM à l'échelle du système.

---

## 6. Ce qui reste ouvert

| Point | État | Bloqué par |
|---|---|---|
| Build CUDA, `sm_89` | non fait | CUDA Toolkit 12.x |
| Temps et VRAM sur ~15 min de français | non fait | CUDA + un fichier de test |
| VAD Silero vérifié | non fait | téléchargement du modèle (~2 Mo) |
| Empaquetage Tauri hors arbre de dev | non fait | CUDA |

**Les temps CPU mesurés ici n'ont aucune valeur prédictive** : ≈ 0,04× temps réel, sans
BLAS, sur un modèle de 1,6 Go. Ils ne servent qu'à établir la correction fonctionnelle.
Le second risque de la phase 00 — l'empaquetage des DLL CUDA par Tauri — reste entier et
est **indépendant** de celui qui vient d'être levé.

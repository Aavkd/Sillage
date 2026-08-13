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

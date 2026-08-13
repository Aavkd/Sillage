# Correctifs appliqués aux dépendances vendorisées

`vendor/whisper-rs-sys` est une copie de **whisper-rs-sys 0.15.0** (crates.io), qui embarque
**whisper.cpp 1.8.3**. Elle est vendorisée uniquement pour porter les correctifs ci-dessous.

Le branchement se fait dans le `Cargo.toml` racine :

```toml
[patch.crates-io]
whisper-rs-sys = { path = "vendor/whisper-rs-sys" }
```

---

## Procédure de mise à jour

**À faire à chaque montée de version de `whisper-rs` ou `whisper-rs-sys`.**

1. Vérifier si le correctif est passé en amont :
   `grep -n "new_segment_callback" whisper.cpp/src/whisper.cpp` dans la nouvelle version.
   Si le site d'appel DTW passe désormais un **compteur** et non un **index**, le correctif
   est inutile : supprimer le dossier vendorisé et le bloc `[patch.crates-io]`.
2. Sinon : recopier la nouvelle version depuis le registre, réappliquer chaque correctif
   ci-dessous, mettre à jour ce fichier (version, numéros de ligne, état amont).
3. Rejouer la vérification de chaque correctif (section « Vérification »).
4. Ne jamais monter de version sans exécuter ces vérifications : le correctif 001 est
   silencieux en cas de régression — le streaming cesse simplement de fonctionner, sans erreur.

---

## PATCH 001 — Callback de streaming inopérant sous DTW

**Fichier** : `whisper.cpp/src/whisper.cpp`
**Version d'origine** : whisper.cpp 1.8.3, ligne 7717
**État amont** : non signalé au 13/08/2026
**Pourquoi il est indispensable ici** : Sillage a besoin **simultanément** des horodatages
mot à mot (DTW) et de l'affichage en streaming. Sans ce correctif, les deux s'excluent.

### Le défaut

whisper.cpp neutralise les deux sites d'appel normaux du callback dès que le DTW est actif
(lignes 7657 et 7702, condition `&& !ctx->params.dtw_token_timestamps`) et les remplace par
un site spécifique au DTW, défectueux à deux titres :

```cpp
for (int seg = (int) result_all.size() - n_segments; seg < n_segments; seg++) {
    params.new_segment_callback(ctx, state, seg, params.new_segment_callback_user_data);
}
```

1. Il transmet un **index de segment** là où tous les autres sites transmettent un
   **nombre de nouveaux segments**. Le trampoline de whisper-rs calcule
   `s0 = total - n_new` : avec `seg = 0` il obtient une plage vide et n'émet rien.
2. La borne `seg < n_segments` compare un index absolu à un compteur. Au-delà du premier
   bloc de 30 s, la condition est fausse d'emblée et le callback ne part jamais.

### Le correctif

```cpp
if (params.new_segment_callback) {
    params.new_segment_callback(ctx, state, n_segments, params.new_segment_callback_user_data);
}
```

Un seul appel, transmettant le **compteur** — la convention de tous les autres sites.

### Vérification

Mesurée sur `spike/fixtures/speech-fr.wav` (10,8 s de français), modèle `large-v3-turbo` :

| | callbacks | couverture DTW |
|---|---|---|
| Avant correctif, DTW activé | **0** | 100 % |
| Avant correctif, DTW désactivé | 1 | 0 % |
| Après correctif, DTW activé | **≥ 1** | 100 % |

Le test à rejouer :

```bash
cd spike && ./target/release/spike.exe \
  --model <ggml-large-v3-turbo.bin> --audio fixtures/speech-fr.wav --lang fr
```

`stream calls` doit être ≥ 1 **et** `dtw coverage` à 100 %. Si l'un des deux tombe,
le correctif a été perdu.

> Vérifier aussi sur un fichier de **plus de 30 s** : le second défaut (borne de boucle)
> ne se manifeste qu'à partir du deuxième bloc. `fixtures/vad-test.wav` (66,6 s) convient.

---

## PATCH 002 — VAD dans le chemin `with_state` — **ANNULÉ, NE PAS REFAIRE**

**Statut** : tenté le 13/08/2026, **annulé le jour même**. Le code est revenu à l'amont.

### Ce qui était visé

whisper.cpp n'implémente le VAD que dans `whisper_full()`, qui délègue ensuite à
`whisper_full_with_state()`. whisper-rs n'appelle **que** la seconde : le drapeau `vad`
est accepté et **silencieusement ignoré**. Le correctif déplaçait le bloc VAD vers
`whisper_full_with_state()`.

### Pourquoi il a échoué

**Segmentation fault systématique**, avec *et* sans DTW.

whisper-rs construit son contexte via `whisper_init_from_file_with_params_no_state` :
**`ctx->state` est nul**. Le chemin VAD de whisper.cpp n'existe que dans `whisper_full()`,
qui n'est jamais appelé qu'avec l'état porté par le contexte — il suppose donc `ctx->state`
valide. Déplacer ce code dans le chemin à état explicite casse cette hypothèse.

Le VAD lui-même fonctionne parfaitement : Silero a détecté la parole à 16,58–24,83 s et
42,40–50,62 s sur `fixtures/vad-test.wav`, dont les plages réelles sont 15,00–25,79 s et
40,79–51,58 s. Le modèle n'est pas en cause, seul l'endroit où on l'appelle l'est.

### Décision retenue

**Faire le VAD côté Rust**, via l'API déjà exportée par whisper-rs (`pub use whisper_vad::*`) :
détecter les plages de parole, transcrire chacune avec un décalage connu, et maîtriser
nous-mêmes le calcul des horodatages. Aucun correctif amont, et cela rejoint le découpage
en blocs que CONCEPTION §8 impose déjà pour les fichiers de plus de 2 h.

> **Ne pas retenter ce correctif** sans avoir d'abord résolu la nullité de `ctx->state`.
> Il compile sans avertissement et plante à l'exécution.

---

## PATCH 003 — Le fork ne se recompilait pas

**Fichier** : `build.rs`
**État amont** : non signalé au 13/08/2026
**Sans ce correctif, tous les autres correctifs sont inopérants.**

### Le défaut, en deux moitiés indépendantes

1. `build.rs` ne déclarait que `cargo:rerun-if-changed=wrapper.h`. Cargo n'honorant **que**
   les chemins déclarés dès lors qu'il y en a, toute modification du C++ vendorisé
   **ne relançait pas** le script de build.
2. Même relancé, le script ne copiait les sources dans `OUT_DIR` que **si le dossier
   n'existait pas**, et cmake compile depuis cette copie. Une modification du fork
   n'atteignait donc jamais le compilateur.

Résultat : `cargo build` réussissait, l'ancienne bibliothèque était réutilisée telle quelle,
et le correctif semblait « ne rien faire ». C'est ce qui a invalidé PATCH 002 pendant
plusieurs cycles de test avant d'être repéré.

PATCH 001 n'a fonctionné que par chance : il était en place lors de la **toute première**
compilation du paquet vendorisé.

### Le correctif

- Déclarer `rerun-if-changed` sur `whisper.cpp/src`, `whisper.cpp/include`,
  `whisper.cpp/ggml/src`, `whisper.cpp/ggml/include`.
- Copier systématiquement les sources dans `OUT_DIR` (`CopyOptions { overwrite: true }`)
  au lieu de ne le faire qu'à la création du dossier.

### Vérification — à faire avant de croire tout résultat de test

```bash
grep -c "SILLAGE PATCH 001" spike/target/release/build/whisper-rs-sys-*/out/whisper.cpp/src/whisper.cpp
```

Un `0` signifie que la copie de build est périmée : **le binaire testé ne contient pas les
correctifs**, quel que soit le code de retour de `cargo build`. En cas de doute :
`rm -rf spike/target/release/build/whisper-rs-sys-*`.

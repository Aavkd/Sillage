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

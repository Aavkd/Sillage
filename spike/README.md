# spike — phase 00

Binaire jetable. Il répond à une seule question : **whisper-rs produit-il des horodatages
mot à mot exploitables pour l'éditeur synchronisé de Sillage ?**

Réponse : **oui**, à condition d'appliquer PATCH 001 (voir [../vendor/PATCHES.md](../vendor/PATCHES.md)).
Détail des mesures : [RESULTS.md](RESULTS.md).

Ce dossier n'est **pas** du code de production et ne sera pas repris tel quel en phase 01.
Il reste dans le dépôt comme preuve reproductible.

---

## Construire

`libclang.dll` est obligatoire (bindgen). Le venv contenu évite d'installer LLVM sur tout
le système :

```bash
python -m venv .venv-libclang
./.venv-libclang/Scripts/python.exe -m pip install libclang
```

```bash
export LIBCLANG_PATH="$PWD/.venv-libclang/Lib/site-packages/clang/native"
cargo build --release
```

> `WHISPER_DONT_GENERATE_BINDINGS=1` **ne fonctionne pas sur Windows** : le `bindings.rs`
> fourni par whisper-rs-sys 0.15.0 est généré sous Linux et casse les assertions de
> layout MSVC.

Pour le GPU, une fois le CUDA Toolkit 12.x installé et `CUDA_PATH` mis à jour :

```bash
cargo build --release --features cuda
```

## Exécuter

```bash
./target/release/spike.exe --model <ggml-large-v3-turbo.bin> --audio <fichier> --lang fr
```

| Option | Effet |
|---|---|
| `--model` | chemin du modèle GGML (requis) |
| `--audio` | fichier audio ou vidéo, tout ce que ffmpeg lit (requis) |
| `--out` | JSON de sortie, défaut `results.json` |
| `--lang` | `fr`, `en`, ou `auto` |
| `--vad` | chemin du modèle Silero GGML, active le VAD |
| `--threads` | défaut : nombre de cœurs |
| `--no-dtw` | désactive le DTW — sert à isoler le conflit streaming/DTW |

## Fixtures

**Non versionnées** : ce sont des enregistrements de la voix de l'utilisateur.
Les régénérer avant de rejouer les vérifications.

```bash
mkdir -p fixtures
cp "<un enregistrement français>" fixtures/speech-fr.wav

# 66,6 s : parole en blocs 1 et 2, silence ailleurs — indispensable pour PATCH 001
ffmpeg -y -i fixtures/speech-fr.wav -f lavfi -t 15 -i anullsrc=r=16000:cl=mono \
  -filter_complex "[1:a][0:a][1:a][0:a][1:a]concat=n=5:v=0:a=1[out]" \
  -map "[out]" -ar 16000 -ac 1 -c:a pcm_s16le fixtures/vad-test.wav
```

Plages de silence de `vad-test.wav`, si la fixture est construite à partir d'un
`speech-fr.wav` de 10,8 s : **0–15 s**, **25,8–40,8 s**, **51,6–66,6 s**.
Aucun mot ne doit y tomber — c'est le contrôle objectif de la qualité d'alignement.

## Ce que le spike ne prouve pas

- **Les temps CPU ne veulent rien dire.** ~0,04× temps réel, sans BLAS, sur un modèle de
  1,6 Go. Seuls les chiffres CUDA comptent.
- **La VRAM n'est pas mesurée** tant que le build CUDA n'existe pas.
- **L'empaquetage Tauri n'est pas testé** : c'est le second risque de la phase 00,
  indépendant de celui-ci.

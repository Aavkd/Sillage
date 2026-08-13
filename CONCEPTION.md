# Sillage — Conception

> Document de conception issu de 6 rounds de questions/réponses.
> A servi d'entrée à la passe de design, désormais réalisée.

**Date** : 13 août 2026
**Auteur** : MANTARA
**Statut** : conception validée · design réalisé · implémentation à venir

**Documents liés**
- [DESIGN.md](DESIGN.md) — jetons, métriques et composants issus de la passe de design
- [ROADMAP.md](ROADMAP.md) — découpage en phases, critères d'acceptation, protocole git
- `app-design-with-glassmorphism/project/Sillage.dc.html` — prototype, fait foi sur l'apparence

---

## 1. Objet

Application desktop Windows permettant de déposer des fichiers audio (et vidéo) et d'en obtenir
une transcription de haute qualité, éditable, avec synchronisation audio↔texte, et des
post-traitements LLM optionnels.

**La transcription est la fonctionnalité phare.** Tout le reste (bibliothèque, LLM, exports)
gravite autour d'elle et ne doit jamais dégrader son expérience.

**Utilisateur** : un seul, sur un seul poste (RTX 4090 Laptop, 16 Go VRAM, Windows 11).
Aucune contrainte de multi-utilisateur, de compte, de quota ou de déploiement distant.
Cela autorise des choix simples : pas d'auth, pas de multi-tenant, CUDA supposé présent.

**Langues** : français majoritaire, anglais supporté.

---

## 2. Journal des décisions

| # | Sujet | Décision |
|---|---|---|
| 1 | Forme | Application desktop **Tauri v2** |
| 2 | Audience | Un seul utilisateur, un seul PC |
| 3 | Sorties | Texte propre + horodatages + post-traitement LLM |
| 4 | Volume | Mémos courts (≤ 15 min) aujourd'hui, enregistrements longs plus tard |
| 5 | Moteur | **whisper.cpp en Rust** (whisper-rs), pas de sidecar Python |
| 6 | Écran d'accueil | **Bibliothèque** (liste consultable + zone de dépôt) |
| 7 | Édition | **Éditeur complet avec synchronisation audio** |
| 8 | Modèle | Sélectionnable dans les réglages : **turbo par défaut**, large en option |
| 9 | Langue | Réglage : **auto-détection / français / anglais** |
| 10 | LLM | **Ollama par défaut**, cloud en option, configurable |
| 11 | Sorties LLM | Version nettoyée, résumé, notes structurées, prompts personnalisés — chacune activable |
| 12 | Stockage | **Dossier bibliothèque géré par l'app** (copie de l'audio) |
| 13 | Pendant le traitement | **Transcription en streaming** + barre de progression + ETA |
| 14 | Chaînage LLM | **Automatique par défaut**, mode « à la demande » en option |
| 15 | Vue transcription | Transcription en héros, panneaux LLM repliables au-dessus |
| 16 | Points d'entrée | Glisser-déposer, sélecteur de fichiers, **menu contextuel Explorer**, **enregistrement direct**, **collage chemin/URL** |
| 17 | Ordonnancement GPU | **Strictement séquentiel**, les deux modèles restent chargés |
| 18 | Obsolescence | **Marquage « obsolète » + régénération en un clic** |
| 19 | Bibliothèque | Titre généré (**éditable**) + date + durée + langue + une ligne de résumé |
| 20 | Organisation | **Tags avec filtrage** |
| 21 | Formats | **Tout audio + vidéo** (piste audio extraite) |
| 22 | Raccourcis | Modificateurs (`Ctrl+Space` interdit — pris par Push-to-talk) |
| 23 | Identité visuelle | À définir lors de la passe de design |
| 24 | Périmètre v1 | **Tout d'un bloc** |

**Principe transverse issu des rounds 3–4** : *les réglages définissent des valeurs par défaut,
jamais des interdits.* Toute option globale (horodatages, résumé, nettoyage, notes) existe aussi
en contrôle par transcription, modifiable à tout moment après traitement. Rien n'est destructif :
l'audio et la structure complète des segments sont toujours conservés.

Corollaire : **la configuration par défaut doit être excellente**, car elle ne sera quasiment
jamais modifiée. Défauts proposés : turbo · auto-détection · Ollama · nettoyage + résumé activés ·
notes et prompts personnalisés désactivés · horodatages activés · chaînage automatique.

---

## 3. Architecture technique

### 3.1 Pile

| Couche | Choix | Notes |
|---|---|---|
| Coque | Tauri v2 | Fenêtre, drag & drop natif, dialogues, notifications, tray |
| Frontend | Web (framework à définir avec le design) | Éditeur, lecteur, bibliothèque |
| Backend | Rust (process principal Tauri) | Pas de sidecar Python |
| STT | whisper.cpp via `whisper-rs`, feature `cuda` | Modèles GGML |
| Décodage audio | ffmpeg **et ffprobe**, embarqués comme ressources | Tout format, extraction piste vidéo |
| Téléchargement URL | yt-dlp, embarqué comme ressource | Point d'entrée « coller une URL » |
| Base | SQLite (`rusqlite`) + FTS5 | Index + recherche plein texte |
| LLM | Ollama HTTP (`localhost:11434`) / API cloud | Abstraction fournisseur |

Plugins Tauri nécessaires : `dialog`, `fs`, `notification`, `single-instance`
(indispensable pour le menu contextuel Explorer : un second lancement doit router le fichier vers
l'instance en cours), `updater` optionnel.

### 3.2 Pipeline de traitement

```
Fichier / URL
    ↓  ingestion — copie dans la bibliothèque, hash SHA-256 (déduplication)
    ↓  ffmpeg   — décodage → PCM 16 kHz mono f32 + extraction des pics de forme d'onde
    ↓  VAD      — Silero (support natif whisper.cpp) : coupe les silences, limite les hallucinations
    ↓  whisper  — transcription avec token_timestamps + DTW (mots) + initial_prompt (vocabulaire)
    │            ↳ callback par segment → événement Tauri → affichage en streaming
    ↓  persist  — segments + mots + probabilités → JSON + index SQLite
    ↓  LLM      — si chaînage auto : nettoyage → résumé → notes (séquentiel)
```

Les pics de forme d'onde sont calculés **pendant** le décodage ffmpeg, à partir du PCM déjà en
mémoire, et stockés avec la transcription. Le frontend ne redécode jamais l'audio pour dessiner
la waveform.

> **Précisé en phase 03.** Le PCM n'est pas seulement calculé au vol, il n'est **jamais
> conservé** : 2 h font 460 Mo, pour un budget de 500 Mo de RSS. Le décodeur remet des blocs à un
> `PcmSink` et les oublie ; la phase 03 y branche le calcul des pics, la phase 04 y branchera
> whisper. Crête mesurée sur 2 h : **8,8 Mo**.
>
> **ffprobe est embarqué en plus de ffmpeg.** Distinguer un fichier corrompu d'une vidéo muette
> demande un contrat lisible par machine ; le JSON de ffprobe en est un, la prose de ffmpeg sur
> stderr n'en est pas un. La durée annoncée par le conteneur n'est en revanche jamais un motif de
> refus : seul le nombre d'échantillons sortis du décodeur fait foi.

### 3.3 Ordonnancement (décision #17)

Une seule file, strictement séquentielle : `transcrire(f1) → LLM(f1) → transcrire(f2) → …`

- Whisper turbo reste résident (~2 Go), laissant ~13 Go à Ollama — confortable pour un modèle 7–14B.
- **Contrainte à documenter dans les réglages** : si l'utilisateur configure Ollama avec un modèle
  > 12 Go, la coexistence n'est plus garantie. L'app doit détecter la taille du modèle Ollama au
  démarrage et avertir, plutôt que de laisser survenir une éviction silencieuse.
- ETA honnête : dérivée de la position audio réelle, jamais d'une barre décorative.

### 3.4 Modèle de données

**Dossier bibliothèque** (emplacement choisi par l'utilisateur, défaut `%USERPROFILE%\Documents\Sillage`) :

```
library/
  library.db                  SQLite : index, métadonnées, tags, FTS5
  media/<id>.<ext>            audio original copié
  data/<id>.json              transcription complète (segments, mots, probabilités)
  data/<id>.peaks             pics de forme d'onde (binaire compact)
  outputs/<id>.<kind>.md      sorties LLM (cleaned, summary, notes, custom:<slug>)
```

**Entrée de transcription** :

```
id, source_path, media_path, sha256, created_at, duration_ms,
title (généré, éditable), title_is_custom,
language (détectée ou forcée), model, status,
segments[ { start_ms, end_ms, text, words[ { id, start_ms, end_ms, text, prob } ] } ],
transcript_hash            ← sert au marquage d'obsolescence
edits[]                    ← corrections utilisateur, séparées du verbatim
tags[]
```

**Sortie LLM** :

```
id, transcript_id, kind, provider, model, prompt_version,
source_transcript_hash, generated_at, content
```

`source_transcript_hash ≠ transcript_hash` ⇒ la sortie est **obsolète** (décision #18).

> **Précisé en phase 02.** `transcript_hash` n'est **pas stocké dans le JSON** : il se dérive du
> texte affiché (verbatim + corrections). Seule la base en garde une copie, puisque c'est là que
> la comparaison d'obsolescence a lieu. Une copie dans le JSON pourrait diverger du texte qu'elle
> est censée décrire ; une valeur dérivée ne le peut pas.

### 3.5 Édition et conservation de la synchronisation

Point délicat : éditer le texte casse la correspondance mot↔temps dont dépend tout l'éditeur.

Approche retenue :

- Le **verbatim est immuable**. Les corrections vivent dans une couche `edits` par-dessus.
- Chaque mot porte un `id` stable. Une correction remplace, insère ou supprime des tokens.
- Un token inséré hérite de l'intervalle temporel de ses voisins (interpolation) — il reste
  cliquable, avec une précision moindre, ce qui est acceptable.
- Un token supprimé disparaît de l'affichage mais son temps reste disponible pour ses voisins.
- **La synchronisation ne se dégrade donc jamais brutalement**, et le verbatim d'origine reste
  restaurable à tout moment (« Revenir au verbatim »).

### 3.6 Risques identifiés

| Risque | Impact | Mitigation |
|---|---|---|
| Horodatages **mot par mot** via DTW dans `whisper-rs` | **Élevé** — tout l'éditeur en dépend | À vérifier en tout premier (voir §10). Vérifier `DtwMode`/`DtwParameters` et l'existence des *alignment heads* pour large-v3-turbo. Repli : synchronisation au **segment** (clic sur une phrase, pas un mot) — nettement moins agréable mais fonctionnel |
| Build CUDA de whisper.cpp | Moyen | Toolkit CUDA requis à la compilation ; DLL cuBLAS/cudart à embarquer. Compter quelques centaines de Mo installés — l'argument « binaire unique de 10 Mo » de Tauri ne tient pas avec CUDA |
| Support VAD Silero dans la version de whisper.cpp retenue | Faible | Fonctionnalité récente : épingler une version de whisper.cpp qui l'inclut, sinon désactiver le VAD (les hallucinations sur silence redeviennent possibles) |
| Contention VRAM Whisper/Ollama | Faible | Séquentiel + avertissement sur les gros modèles (§3.3) |
| Hallucinations sur silence / musique | Moyen | VAD + seuils `no_speech` + signalement visuel des passages à faible confiance |

---

## 4. Réglages

L'application étant entièrement pilotée par les réglages, voici la spécification complète.
Organisation en sections ; chaque réglage a une valeur par défaut utilisable sans jamais ouvrir cet écran.

### Transcription
- **Modèle** — `large-v3-turbo` (défaut) · `large-v3`
- **Langue** — Auto-détection (défaut) · Français · Anglais
- **Vocabulaire personnalisé** — liste de termes (noms propres, clients, jargon) injectés en
  `initial_prompt`. *Très fort levier de qualité en français pour un coût nul.*
- **VAD** — activé (défaut)
- **Horodatages** — activés par défaut sur les nouvelles transcriptions

### Post-traitement
- **Mode** — Automatique (défaut) · À la demande
- **Fournisseur** — Ollama (défaut) · Cloud
  - Ollama : URL (`localhost:11434`), modèle
  - Cloud : fournisseur, clé API, modèle
- **Actions activées** — Version nettoyée ✓ · Résumé ✓ · Notes structurées ✗ · Prompts personnalisés ✗
- **Prompts personnalisés** — création, édition, suppression, réordonnancement

### Bibliothèque
- **Emplacement du dossier** (déplacement avec migration)
- **Conserver l'audio d'origine** — oui (défaut)
- **Dossier surveillé** — désactivé par défaut *(non retenu au round 4, prévu comme extension)*

### Apparence
- **Couleur d'accent** — roue chromatique + 5 préréglages + saisie hex.
  L'accent re-teinte l'application entière en direct : fond maillé, bordures pointillées,
  barres lues de la forme d'onde, vumètre, et bascule automatique de la couleur du texte
  posé sur l'accent. Spécification complète : [DESIGN.md](DESIGN.md) §3 et §12.
- **Thème** — Sombre (défaut) · Clair
- **Langue de l'interface** — français
- **Raccourcis** — affichage de la liste ; personnalisation non prévue en v1

### Système
- **Menu contextuel Explorer** — inscription/désinscription
- **Lancement au démarrage**
- **Modèles installés** — taille, emplacement, téléchargement, suppression

---

## 5. Écrans et états

> Section principale pour la passe de design. Le contenu, la hiérarchie et les états sont
> spécifiés ; la typographie, les couleurs, l'espacement et le langage visuel sont **ouverts**.

### 5.1 Bibliothèque (accueil)

**Contenu**
- Zone de dépôt en haut — cible de glisser-déposer, bouton de sélection de fichiers,
  bouton d'enregistrement, champ de collage chemin/URL.
- Barre de recherche (plein texte : transcriptions + résumés + titres).
- Filtre par tags.
- Liste des transcriptions, triée par date décroissante.

**Carte de transcription**
- Titre généré, **éditable en ligne** (un clic sur le titre suffit)
- Date · durée · langue détectée · modèle utilisé
- Une ligne de résumé (repli : première phrase du verbatim)
- Tags
- État si en cours : progression, ETA, aperçu du texte en streaming

**États à concevoir**
- **Vide (premier lancement)** — aucune transcription. Doit expliquer les points d'entrée sans être un tutoriel.
- **Premier lancement, modèle absent** — téléchargement du modèle avec progression, taille, possibilité d'annuler. L'app reste utilisable (navigation) pendant le téléchargement.
- **En cours** — une ou plusieurs cartes en traitement, une seule active (séquentiel), les autres en file d'attente avec position.
- **Erreur sur un fichier** — la carte porte l'erreur, ne bloque pas la file, propose « Réessayer ».
- **Recherche sans résultat**
- **Filtre par tag sans résultat**

### 5.2 Détail d'une transcription

**Disposition** (décision #15) : une seule colonne, un seul défilement.

1. **En-tête** — titre éditable, date, durée, langue, modèle, tags, actions (exporter, supprimer, re-transcrire)
2. **Lecteur** — épinglé en haut lors du défilement. Forme d'onde, position, durée, vitesse de lecture, volume.
3. **Panneaux LLM repliables** — Résumé · Version nettoyée · Notes · prompts personnalisés.
   Chaque panneau : contenu, fournisseur+modèle utilisés, date de génération, actions
   (régénérer, copier, exporter), et **badge « obsolète »** si le verbatim a changé depuis.
   Un panneau désactivé dans les réglages apparaît **replié avec un bouton « Générer »** —
   jamais absent (principe du §2).
4. **Transcription** — le héros. Pleine largeur, lisible sur la durée.

**Comportement de la transcription**
- Le mot en cours de lecture est mis en évidence ; le texte défile automatiquement (désactivable dès que l'utilisateur défile manuellement).
- Clic sur un mot → lecture depuis ce mot.
- Édition en ligne, directement dans le flux, sans mode « édition » séparé.
- Horodatages affichables/masquables (bascule locale, sans re-transcription).
- **Mots à faible confiance** subtilement signalés (proposition, §9) — pour savoir où vérifier.

**États à concevoir**
- **En cours de transcription** — texte apparaissant progressivement, progression + ETA, lecteur déjà utilisable
- **Terminé, LLM en cours** — transcription complète, panneaux LLM en chargement
- **Sortie LLM obsolète** — après édition
- **Édité** — indicateur de divergence avec le verbatim + action « Revenir au verbatim »
- **Fichier média manquant** — ne devrait pas arriver (l'app possède la copie), mais prévoir le cas
- **Échec LLM** — Ollama éteint, clé API invalide, modèle absent : message actionnable, la transcription reste intacte

### 5.3 Enregistrement

Modale ou vue dédiée : sélection du micro, niveau d'entrée visible, durée, pause, arrêt.
À l'arrêt, l'enregistrement entre dans la file comme n'importe quel fichier.

> Recouvrement assumé avec Push-to-talk, qui fait déjà de la capture micro. Ici l'usage est
> différent : enregistrement long archivé dans la bibliothèque, pas dictée injectée au clavier.

### 5.4 Réglages

Structure du §4. Un écran, sections ancrées.
Chaque changement s'applique immédiatement (pas de bouton « Enregistrer »).

### 5.5 Import par URL

Collage d'une URL → résolution via yt-dlp → affichage du titre, de la durée et de la source
**avant** téléchargement → confirmation → entrée dans la file.

---

## 6. Interactions

### Raccourcis (décision #22 — `Ctrl+Space` réservé à Push-to-talk)

| Raccourci | Action |
|---|---|
| `Ctrl+Shift+Space` | Lecture / pause |
| `Ctrl+←` / `Ctrl+→` | Reculer / avancer de 5 s |
| `Ctrl+↑` / `Ctrl+↓` | Vitesse de lecture |
| `Ctrl+F` | Rechercher dans la transcription |
| `Ctrl+Shift+F` | Rechercher dans la bibliothèque |
| `Ctrl+S` | Exporter |
| `Échap` | Retour à la bibliothèque |

Tous utilisables sans quitter le curseur texte.

### Glisser-déposer
Dépôt possible n'importe où dans la fenêtre, pas seulement sur la zone dédiée.
Survol → état visuel explicite. Dépôt multiple → mise en file dans l'ordre de dépôt.

### Menu contextuel Explorer
« Transcrire avec Transcript » sur les extensions audio et vidéo supportées.
Clé de registre posée par l'installeur, désinscrivable depuis les réglages.
Instance unique : le fichier est routé vers l'app déjà ouverte.

### Notifications
Toast système en fin de traitement **uniquement si la fenêtre n'a pas le focus**.
Pas de notification quand l'utilisateur regarde déjà l'écran.

---

## 7. Exports

*(Non couvert par les rounds — proposition, à valider.)*

| Format | Contenu |
|---|---|
| `.txt` | Texte seul |
| `.md` | Titre, métadonnées, sorties LLM activées, transcription |
| `.docx` | Idem, mis en forme |
| `.srt` / `.vtt` | Sous-titres depuis les segments |
| `.json` | Structure complète (segments, mots, probabilités) |
| Presse-papiers | Texte seul ou avec horodatages |

Une case « inclure les sorties LLM » dans le dialogue d'export.
L'export reflète toujours les corrections, jamais le verbatim d'origine — sauf choix explicite.

---

## 8. Cas limites

| Cas | Comportement attendu |
|---|---|
| Fichier corrompu / illisible | Rejet à l'ingestion avec l'erreur ffmpeg traduite en langage clair |
| Fichier sans piste audio (vidéo muette) | Rejet explicite |
| Fichier de durée nulle ou < 1 s | Rejet explicite |
| Doublon (même SHA-256) | Signalé, avec le choix « ouvrir l'existante » ou « transcrire à nouveau » |
| Fichier encore en cours d'écriture | Attente de stabilisation de la taille avant ingestion |
| Langue mal détectée | Visible sur la carte ; correction + re-transcription en un clic |
| Modèle absent au moment du traitement | Téléchargement automatique avec progression, file en pause |
| Ollama injoignable | Transcription conservée, panneaux LLM en erreur actionnable |
| VRAM insuffisante | Message explicite avec suggestion (modèle plus petit), jamais un crash |
| Fermeture pendant un traitement | Confirmation ; la file est persistée et reprise au lancement suivant |
| Fichier très long (> 2 h) | Doit fonctionner : traitement par blocs, écriture incrémentale, reprise possible |

---

## 9. Hypothèses et questions ouvertes

**Décisions prises par défaut** — à corriger si elles ne conviennent pas :

1. **Langue de l'interface : français**, par cohérence avec Push-to-talk.
2. **Exports** : formats du §7.
3. **Mots à faible confiance signalés visuellement** — Whisper fournit une probabilité par mot ;
   l'exploiter indique où relire. Discret par défaut, désactivable.
4. **Vocabulaire personnalisé via `initial_prompt`** (§4) — fort levier de qualité en français
   sur les noms propres, coût quasi nul.
5. **Bouton « Re-transcrire en haute précision »** sur chaque transcription : puisque l'audio est
   conservé, passer de turbo à large-v3 sur une transcription décevante ne coûte qu'un clic.
   Le réglage global reste celui de la décision #8 ; ceci en est le pendant par transcription,
   conformément au principe du §2.
6. **Dossier surveillé** conservé comme réglage désactivé (non retenu au round 4, mais l'architecture
   le rend trivial à activer).

**Tranché depuis**

- **Nom de l'application** : **Sillage**.
- **Identité visuelle** : passe de design réalisée — verre chaud, thème sombre par défaut,
  thème clair complet, accent choisi par l'utilisateur. Voir [DESIGN.md](DESIGN.md).
- **Dépôt** : `https://github.com/Aavkd/Sillage.git`

**Tranché en phase 01**

- **Framework frontend** : **React 19 + TypeScript + Vite**, choisi par l'utilisateur.
  Motif retenu : l'écosystème le plus fourni pour ce qui vient — virtualisation d'une
  transcription de 2 h (phase 06), outillage de test mature, défaut documenté de Tauri v2.
- **Polices** : paquets `@fontsource-variable`, woff2 servis depuis le bundle local.

**Reste à trancher**

- Rien à ce stade.

---

## 10. Ordre de construction

Périmètre v1 : **tout d'un bloc** (décision #24). L'ordre ci-dessous ne découpe pas la livraison,
il ordonne le travail pour que l'inconnue technique majeure soit levée au premier jour.

1. **Spike (à faire en premier)** — `whisper-rs` + CUDA + DTW sur un fichier réel, en Rust seul.
   Objectif unique : **obtenir des horodatages mot par mot exploitables**, et vérifier que le
   build CUDA s'empaquette dans Tauri. Tout l'éditeur en dépend ; si le repli au segment s'impose,
   il vaut mieux le savoir avant d'avoir dessiné l'éditeur.
2. Squelette Tauri + coque + SQLite/FTS5 + dossier bibliothèque.
3. Ingestion : ffmpeg, formats, pics de forme d'onde, déduplication, gestion des erreurs.
4. Transcription en streaming : callback segment → événements → file séquentielle persistée.
5. Bibliothèque : cartes, recherche, tags, titres éditables.
6. Vue détail : lecteur, forme d'onde, synchronisation mot par mot.
7. Éditeur : couche de corrections, retour au verbatim, hash d'obsolescence.
8. Couche LLM : abstraction fournisseur, Ollama, cloud, les quatre types de sorties, régénération.
9. Points d'entrée : menu contextuel Explorer, instance unique, enregistrement micro, URL.
10. Exports.
11. Réglages complets, téléchargement des modèles, premier lancement.
12. Finition : raccourcis, notifications, états vides, états d'erreur, cas limites du §8.

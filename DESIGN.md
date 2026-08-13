# Sillage — Référence de design

> **Source de vérité visuelle** : `app-design-with-glassmorphism/project/Sillage.dc.html`
> Ce document en extrait toutes les valeurs numériques pour permettre une implémentation
> pixel-perfect sans re-dériver le prototype à chaque fois.
>
> **En cas de divergence entre ce document et le prototype, le prototype fait foi.**
> Signaler la divergence et corriger ce document.

---

## 0. Ce que « pixel-perfect » veut dire ici

**S'applique** : dimensions, espacements, rayons, épaisseurs de bordure, tailles et graisses de
police, interlignes, letter-spacing, opacités, valeurs de flou, ombres, couleurs, ordre et
hiérarchie des éléments.

**Ne s'applique pas** : le contenu d'exemple. « 809 Mo », « Micro Yeti », « Entretien Marchand »,
les 24 transcriptions, les formes d'onde générées par sinus — tout cela est du remplissage.
L'implémentation affiche les vraies valeurs. (Le vrai `ggml-large-v3-turbo.bin` pèse 1,62 Go,
pas 809 Mo.)

**Méthode de vérification** : comparer chaque valeur au tableau de ce document, propriété par
propriété, dans l'inspecteur. Pas de comparaison d'images : le prototype utilise des polices
Google Fonts chargées en ligne et des données aléatoires, un diff visuel produira du bruit.

---

## 1. Polices

| Rôle | Police | Graisses | Usage |
|---|---|---|---|
| Interface | **Figtree** | 400, 500, 600, 700 | Tout le châssis : titres, boutons, libellés, cartes |
| Lecture | **Newsreader** | 400, 500 (`opsz` 6..72) | Transcription, résumés, texte en streaming |
| Technique | **JetBrains Mono** | 400, 500 | Horodatages, métadonnées, valeurs, raccourcis, hex |

Les trois polices doivent être **embarquées dans l'application** (fichiers woff2 locaux), jamais
chargées depuis Google Fonts : l'app fonctionne hors ligne.

`font-family` de repli : `Figtree, Helvetica, sans-serif`.

---

## 2. Jetons de couleur

### 2.1 Thème sombre (défaut)

```
--page          #120C08
--frame         #1A120C
--text          #F7EEE5
--dim           rgba(247,238,229,.62)
--faint         rgba(247,238,229,.40)
--panel         rgba(255,240,226,.055)
--panelStrong   rgba(255,240,226,.09)
--border        rgba(255,232,210,.13)
--hair          rgba(255,232,210,.08)
--sunken        rgba(0,0,0,.24)
--sel           rgba(255,240,226,.10)
--dropBg        rgba(255,240,226,.035)
--warn          #E8B15C
--warnBorder    rgba(232,177,92,.42)
--err           #E58367
--errBorder     rgba(229,131,103,.42)
--errSoft       rgba(229,131,103,.16)
--ok            #93C79A
```

### 2.2 Thème clair

```
--page          #F4EAE0
--frame         #FBF4EC
--text          #241811
--dim           rgba(36,24,17,.62)
--faint         rgba(36,24,17,.42)
--panel         rgba(255,252,248,.62)
--panelStrong   rgba(255,252,248,.82)
--border        rgba(90,55,30,.14)
--hair          rgba(90,55,30,.09)
--sunken        rgba(90,55,30,.06)
--sel           rgba(90,55,30,.08)
--dropBg        rgba(255,252,248,.5)
--warn          #9A6512
--warnBorder    rgba(154,101,18,.35)
--err           #B4472A
--errBorder     rgba(180,71,42,.35)
--errSoft       rgba(180,71,42,.14)
--ok            #3F7A49
```

### 2.3 Jetons dérivés de l'accent

Recalculés à chaque changement d'accent **ou** de thème :

```
--accent       la couleur choisie                       défaut #E08A4B
--accentSoft   mix(accent, .16) sombre | .18 clair
--dashed       mix(accent, .38) sombre | .45 clair
--onAccent     lum(accent) > 0.62 ? #241811 : #FFF8F1
```

Fonctions de référence, à reproduire à l'identique :

```js
mix(hex, alpha)  // → rgba(r, g, b, alpha)   composantes de hex, alpha tel quel
lum(hex)         // → (0.299·R + 0.587·G + 0.114·B) / 255
```

`--onAccent` est la couleur du texte posé **sur** l'accent (bouton « Enregistrer », bouton
lecture, boutons primaires des modales). Le seuil 0.62 est volontairement haut : il bascule en
texte sombre dès que l'accent devient clair.

### 2.4 Fond maillé (`--mesh`)

Trois dégradés radiaux superposés, en `position: absolute; inset: 0; pointer-events: none`
au-dessus de `--frame` et sous le contenu.

**Sombre**
```
radial-gradient(900px 520px at 12% -8%,  mix(accent,.20),      transparent 68%),
radial-gradient(760px 500px at 96%  6%,  rgba(120,70,40,.22),  transparent 70%),
radial-gradient(700px 600px at 60% 110%, mix(accent,.10),      transparent 72%)
```

**Clair**
```
radial-gradient(900px 520px at 10% -10%, mix(accent,.26),      transparent 66%),
radial-gradient(760px 520px at 98%   4%, rgba(230,190,150,.45),transparent 70%),
radial-gradient(700px 600px at 55% 112%, mix(accent,.14),      transparent 72%)
```

Le maillage réagit donc à l'accent : changer la couleur re-teinte toute la fenêtre. C'est l'effet
principal de la roue, il doit être immédiat et sans rechargement.

### 2.5 Accents prédéfinis

Les 5 pastilles, dans cet ordre :

```
#E08A4B   #D9694E   #C98A2E   #B9755F   #8E9A5B
```

`#E08A4B` est le défaut.

---

## 3. Roue d'accent

Le prototype produit délibérément une **bande de couleurs contrainte** : ni néon, ni sombre.
La formule doit être reproduite exactement, sinon la roue casse l'harmonie de l'app.

```js
// disque de rayon r, centre (r, r)
h = (atan2(dy, dx) · 180/π + 360) mod 360      // teinte = angle
s = min(1, d / r)                              // distance normalisée au centre
couleur = hsl(h, 0.55·s + 0.12, 0.60 − 0.06·s)
```

Conséquences à préserver : saturation bornée à **12 %–67 %**, luminosité bornée à **54 %–60 %**.
Le centre est un gris chaud, le bord la couleur la plus saturée disponible.

**Rendu** : canvas de **320 × 320** px internes, affiché en **160 × 160** px (rendu 2×, net sur
écran HiDPI), `border-radius: 50%`, `cursor: crosshair`.
Anti-aliasing du bord : `alpha = d > r − 1.5 ? 255·(r − d)/1.5 : 255`.

**Clic** : convertir les coordonnées client en coordonnées canvas via `getBoundingClientRect()`,
ignorer si `d > r`, appliquer la formule ci-dessus.

Conversion HSL→RGB de référence (variante sans division par 360) :
```js
k = n => (n + h/30) % 12
a = s · min(l, 1 − l)
f = n => l − a · max(−1, min(k(n) − 3, min(9 − k(n), 1)))
rgb = [f(0), f(8), f(4)].map(v => round(v · 255))
```

---

## 4. Verre

`backdrop-filter` par famille de composant. Valeurs littérales du prototype, à respecter :

| Composant | backdrop-filter |
|---|---|
| Carte latérale « Moteur », états, cartes 14px | `blur(18px)` |
| Zone de dépôt, barre de recherche | `blur(18px)` |
| Cartes de transcription, panneaux LLM, panneaux réglages | `blur(20px)` |
| Carte de transcription en cours | `blur(20px) saturate(1.3)` |
| Carte d'accent (spécimen d'en-tête) | `blur(24px) saturate(1.3)` |
| Lecteur audio | `blur(26px) saturate(1.4)` |
| Modales | `blur(30px) saturate(1.4)` |

> Le prototype expose un `--blur` (défaut 22, plage 0–40) hérité de l'outil de design, mais
> **ne l'utilise nulle part** : chaque panneau porte une valeur littérale. Ne pas exposer ce
> réglage à l'utilisateur ; utiliser les valeurs du tableau.

---

## 5. Ombres

```
Cadre d'écran      0 40px 90px -40px rgba(20,8,0,.8), 0 0 0 1px var(--border)
Carte d'accent     0 24px 60px -30px rgba(20,8,0,.7)
Lecteur            0 20px 44px -28px rgba(20,8,0,.75)
Modale             0 30px 70px -34px rgba(20,8,0,.85)
Carte en cours     0 20px 50px -30px var(--accent)
Bouton accent      0 10px 26px -12px var(--accent)
Bouton lecture     0 8px 22px -8px var(--accent)
Pastille accent    0 0 0 1px var(--border), 0 6px 18px -6px var(--accent)
Pastille rec       0 0 0 6px var(--errSoft)
```

---

## 6. Fenêtre et châssis

- Fenêtre principale : **1360 × 900** (bibliothèque), **1360 × 980** (détail — même fenêtre,
  contenu plus haut ; la fenêtre reste redimensionnable, 1360×900 est la taille par défaut).
- Décorations natives **désactivées**. Barre de titre custom, hauteur **46 px**.
- Rayon du cadre : 22 px.
- Barre de titre bibliothèque : padding `0 18px`, zone de drag (`data-tauri-drag-region`).
  - Gauche : pastille 9×9 px `border-radius: 50%` en `--accent`, puis « Sillage » 13px/600, `letter-spacing: .02em`
  - Droite : `— ▢ ✕`, JetBrains Mono 15px, `--dim`, `gap: 18px`
- Barre de titre détail : `← Bibliothèque` 14px `--dim` à gauche, contrôles à droite.

---

## 7. Écran 01 — Bibliothèque (1360 × 900)

### Grille
`grid-template-columns: 232px 1fr`, hauteur pleine, `min-height: 0`.

### Barre latérale (232 px)
`padding: 14px 16px 20px`, `gap: 22px`, `border-right: 1px solid var(--border)`.

**Navigation** — `gap: 4px`, items `padding: 9px 12px; border-radius: 11px; font-size: 14px`
| Item | État | Compteur |
|---|---|---|
| Bibliothèque | actif : `background: var(--sel)`, `font-weight: 500`, texte plein | `24` mono 12px `--dim` |
| File d'attente | `--dim` | `3` mono 12px |
| Réglages | `--dim` | — |

Les compteurs sont `justify-content: space-between` dans l'item.

**Tags** — libellé `11px`, `letter-spacing: .16em`, `text-transform: uppercase`, `--dim`,
`margin-bottom: 10px`. Pastilles `font-size: 12.5px; padding: 5px 10px; border-radius: 999px`,
`gap: 6px`, `flex-wrap: wrap`.
- Actif : `border: 1px solid var(--accent); color: var(--accent); background: var(--accentSoft)`
- Inactif : `border: 1px solid var(--border); color: var(--dim)`

**Carte Moteur** — `margin-top: auto`, `background: var(--panel)`, `border: 1px solid var(--border)`,
`border-radius: 14px`, `padding: 12px 13px`, `blur(18px)`.
- Titre « Moteur » 12px `--dim`, `margin-bottom: 6px`
- Lignes 13px `space-between` : nom du modèle à gauche, état à droite en `--ok`
  (`large-v3-turbo` / `chargé`, `llama3.1:8b` / `prêt`), 2ᵉ ligne `margin-top: 3px`

### Zone principale
`padding: 14px 22px 20px`, `gap: 16px`.

**Barre de recherche + Enregistrer** — `gap: 12px`
- Champ : `flex: 1`, hauteur **40 px**, `padding: 0 14px`, `border-radius: 12px`,
  `background: var(--panel)`, `border: 1px solid var(--border)`, `blur(18px)`, `gap: 10px`
  - icône `⌕` 14px `--dim`, placeholder 14px `--dim`
  - à droite : `Ctrl+Shift+F` mono 11px `--dim`, `margin-left: auto`
- Bouton : hauteur **40 px**, `padding: 0 16px`, `border-radius: 12px`,
  `background: var(--accent)`, `color: var(--onAccent)`, 14px/600, ombre bouton accent

**Zone de dépôt** — `border: 1.5px dashed var(--dashed)`, `border-radius: 16px`,
`background: var(--dropBg)`, `blur(18px)`, `padding: 20px 22px`, `gap: 22px`
- Icône : 46×46, `border-radius: 14px`, `background: var(--accentSoft)`,
  `border: 1px solid var(--accent)`, `color: var(--accent)`, `↓` 20px, centrée
- Titre 15px/600 « Déposez un fichier audio ou vidéo »
- Sous-titre 13.5px `--dim`, `margin-top: 3px` : « n'importe où dans la fenêtre · ou
  *parcourir* · ou collez un chemin / une URL » — « parcourir » en `--accent`
- Champ URL : hauteur 36px, `padding: 0 14px`, `border-radius: 10px`,
  `border: 1px solid var(--border)`, `background: var(--sunken)`, 13.5px `--dim`,
  `min-width: 240px`, placeholder `https://…`

**Liste des cartes** — `gap: 10px`

*Carte en cours* : `border: 1px solid var(--accent)`, `border-radius: 16px`,
`padding: 16px 18px`, `blur(20px) saturate(1.3)`, ombre carte en cours
- Ligne 1 (`gap: 10px`) : point 7×7 accent · titre 16px/600 · badge `EN COURS` (mono 11px,
  `border: 1px solid var(--accent)`, `border-radius: 6px`, `padding: 2px 6px`, `--accent`) ·
  ETA à droite (mono 12.5px `--dim`)
- Barre de progression : hauteur **4 px**, `border-radius: 999px`, fond `--sunken`,
  remplissage `--accent`, `margin: 12px 0 10px`
- Texte en streaming : **Newsreader 15px / 1.6**, `--dim` pour le texte établi,
  `--text` pour les derniers mots, curseur = `<span>` inline-block 8×15 px en `--accent`,
  `vertical-align: -2px`, `margin-left: 3px`
- Métadonnées : mono 12px `--dim`, `gap: 14px`, `margin-top: 12px` — date · durée · langue · modèle

*Carte normale* : `border: 1px solid var(--border)`, `border-radius: 16px`, `padding: 15px 18px`,
`blur(20px)`
- Ligne 1 : titre 16px/600 · « modifier » 12px `--dim` avec `border-bottom: 1px dashed var(--dim)`
  · tags à droite (`margin-left: auto`, `gap: 6px`, pastilles 12px `padding: 3px 9px`)
- Résumé 14.5px `--dim`, `margin-top: 6px`, `text-wrap: pretty`
- Métadonnées mono 12px **`--faint`**, `gap: 14px`, `margin-top: 10px`

*Carte en échec* : identique mais `border: 1px solid var(--errBorder)`, badge `ÉCHEC`
(mono 11px `--err`, `border: 1px solid var(--errBorder)`), « Réessayer » 13.5px `--accent` à droite,
message 14.5px `--dim`.

*Ligne d'attente* : `padding: 12px 18px`, `border-radius: 16px`,
`border: 1px dashed var(--border)`, `--dim` 14px, `gap: 12px` —
compteur mono 12px puis liste des noms de fichiers.

---

## 8. Écran 02 — Détail (1360 × 980)

Colonne centrée de **880 px**, `gap: 18px`, `padding-bottom: 24px`, défilement vertical.

### En-tête
- Titre **30px / 600 / letter-spacing -.02em**, éditable au clic
- Indication « titre éditable » 12.5px `--dim`, `border-bottom: 1px dashed var(--dim)`
- Actions à droite, `gap: 8px`, chacune `padding: 7px 12px`, `border-radius: 10px`,
  `border: 1px solid var(--border)`, `background: var(--panel)`, 13.5px `--dim` :
  « Exporter », « Re-transcrire en large-v3 »
- Ligne méta `margin-top: 8px`, mono 12.5px `--faint`, `gap: 14px` : date · durée · langue · modèle,
  puis pastilles de tags (Figtree 12px, `padding: 3px 9px`, `border-radius: 999px`),
  puis à droite « Édité · revenir au verbatim » Figtree 12.5px **`--warn`**

### Lecteur
`background: var(--panelStrong)`, `border: 1px solid var(--border)`, `border-radius: 18px`,
`padding: 14px 18px`, `blur(26px) saturate(1.4)`, ombre lecteur, `gap: 16px`
- Bouton lecture : 40×40, `border-radius: 50%`, `background: var(--accent)`,
  `color: var(--onAccent)`, 14px, ombre bouton lecture
- Forme d'onde : `flex: 1`, hauteur **46 px**, `align-items: flex-end`, `gap: 2px`,
  **96 barres**, chacune `flex: 1`, `border-radius: 2px`
  - lue : `--accent`
  - non lue : `mix(#FFF0E2, .22)` en sombre, `mix(#3A2617, .22)` en clair
- Temps : mono 13px `--dim`, `min-width: 104px`, `text-align: right`, format `08:14 / 42:18`
- Pastilles vitesse `1.0×` et volume `♪` : mono 12.5px, `padding: 4px 8px`,
  `border-radius: 8px`, `border: 1px solid var(--border)`, `--dim`, `gap: 8px`

### Panneaux LLM
`gap: 10px`. Base commune : `background: var(--panel)`, `border: 1px solid var(--border)`,
`border-radius: 16px`, `blur(20px)`.

*Déplié* — `padding: 15px 18px`
- Chevron `▾` 12px `--dim` · titre 15px/600 · badge d'état · actions à droite
- Badge `OBSOLÈTE` : mono 11px, `padding: 2px 7px`, `border-radius: 6px`,
  `border: 1px solid var(--warnBorder)`, `color: var(--warn)`
- « Régénérer » 13px `--accent`, « Copier » 13px `--dim`
- Contenu : **Newsreader 16.5px / 1.65**, `margin-top: 10px`, `text-wrap: pretty`
- Pied : mono 11.5px `--faint`, `margin-top: 10px` —
  `ollama · llama3.1:8b · généré le 12 août 16:44 · le verbatim a changé depuis`

*Replié* — `padding: 14px 18px`, une seule ligne : `▸` · titre 15px/600 ·
méta mono 11.5px `--faint` · « Déplier » 13px `--dim` à droite

*Désactivé dans les réglages* — titre en `--dim`, mention « désactivé dans les réglages »
12.5px `--faint`, bouton « Générer » à droite : 13px, `padding: 6px 12px`, `border-radius: 9px`,
`border: 1px solid var(--accent)`, `color: var(--accent)`, `background: var(--accentSoft)`

*En erreur* — `border: 1px solid var(--errBorder)`, chevron en `--err`, message 13px `--err`,
« Réessayer » 13px `--accent`. Le titre peut porter un qualificatif : `Plan d'action`
suivi de `prompt personnalisé` en `--faint`, `font-weight: 400`

### Séparateur de section
`margin-top: 6px`, `gap: 12px` : libellé « Transcription » 13px, `letter-spacing: .16em`,
`text-transform: uppercase`, `--dim` · trait `flex: 1; height: 1px; background: var(--border)` ·
bascules mono 12px, `padding: 4px 9px`, `border-radius: 8px` :
- active (`horodatages`) : `border: 1px solid var(--accent)`, `color: var(--accent)`
- inactive (`confiance`) : `border: 1px solid var(--border)`, `color: var(--dim)`

### Transcription
`gap: 20px` entre paragraphes. **Newsreader 19.5px / 1.78**.
Chaque paragraphe : `display: flex; gap: 20px`
- Horodatage : mono 12.5px, `min-width: 52px`, `padding-top: 8px`
  - segment courant : `--accent` · autres : `--faint`
- Texte : `text-wrap: pretty`
  - segment courant : `--text` (pleine intensité)
  - autres : **`--dim`**

**Mot en cours de lecture** : `background: var(--accentSoft)`,
`box-shadow: inset 0 -2px 0 var(--accent)`, `border-radius: 3px`, `padding: 1px 2px`

**Mot à faible confiance** : `border-bottom: 2px dotted var(--warn)`

**Marqueur de correction** : pastille inline `corrigé`, Figtree 12px, `--warn`,
`border: 1px solid var(--warnBorder)`, `border-radius: 6px`, `padding: 1px 7px`,
`margin-left: 8px`, `vertical-align: 3px`

> L'atténuation des segments non courants (`--dim`) et l'illumination du segment courant
> (`--text`) est le principal repère de lecture pendant la lecture audio. Ne pas l'omettre.

---

## 9. Écran 03 — Réglages (660 × 760)

`padding: 22px 26px`, `gap: 18px`.
- Titre « Réglages » 24px/600/-.02em
- Onglets : `gap: 8px`, 13px `--dim`, `padding: 5px 11px`, actif :
  `border-radius: 999px; background: var(--sel); color: var(--text)`
  → **Transcription · Post-traitement · Bibliothèque · Système** *(+ Apparence, voir §12)*
- Bloc de réglages : `background: var(--panel)`, `border: 1px solid var(--border)`,
  `border-radius: 16px`, `blur(20px)`, `padding: 4px 16px`
  - Ligne : `padding: 13px 0`, `border-bottom: 1px solid var(--hair)`, `gap: 16px`
    - libellé 14.5px/500 · indice 12.5px `--faint` `margin-top: 2px`
    - valeur à droite : mono 12.5px `--accent`, `padding: 5px 11px`, `border-radius: 9px`,
      `border: 1px solid var(--border)`, `background: var(--sunken)`
- Carte d'avertissement : `border: 1px solid var(--warnBorder)`, `padding: 14px 16px`,
  titre 14px/600 `--warn`, corps 13.5px `--dim` `margin-top: 4px`
- Carte « Vocabulaire personnalisé » : titre 14px/600 `margin-bottom: 10px`,
  pastilles 12.5px `padding: 4px 10px` `border-radius: 999px` `border: 1px solid var(--border)`
  `--dim` `background: var(--sunken)`, `gap: 6px`, plus une pastille
  `+ ajouter` en `border: 1px dashed var(--accent)`, `color: var(--accent)`

---

## 10. Écran 04 — Modales (620 × 760)

Base commune : `background: var(--panelStrong)`, `border: 1px solid var(--border)`,
`border-radius: 20px`, `padding: 24px`, `blur(30px) saturate(1.4)`, ombre modale.

### Enregistrement
- Titre 18px/600 · sous-titre 13.5px `--dim` `margin-top: 4px` (périphérique actif)
- Vumètre : hauteur **60 px**, `gap: 3px`, `align-items: flex-end`, **44 barres**,
  `flex: 1`, `border-radius: 2px`, `margin: 20px 0 14px`
  - barres actives `mix(accent, .75)` · barres inactives `mix(accent, .22)`
- Ligne de contrôle `gap: 14px` :
  - pastille d'enregistrement 44×44 `border-radius: 50%` `background: var(--err)`,
    `box-shadow: 0 0 0 6px var(--errSoft)`
  - chrono mono **24 px**, `letter-spacing: .02em`
  - à droite : « Pause » (`padding: 8px 14px`, `border-radius: 10px`,
    `border: 1px solid var(--border)`, `--dim`, 13.5px) et
    « Arrêter et transcrire » (même métrique, `background: var(--accent)`,
    `color: var(--onAccent)`, `font-weight: 600`)

### Import par URL
- Titre 18px/600
- Champ URL : mono 13px `--dim`, `margin-top: 12px`, `padding: 10px 12px`,
  `border-radius: 10px`, `background: var(--sunken)`, `border: 1px solid var(--border)`,
  `text-overflow: ellipsis`, `white-space: nowrap`
- Aperçu `gap: 14px` `margin-top: 16px` :
  vignette 74×74 `border-radius: 12px` `background: var(--sunken)`
  `border: 1px solid var(--border)` ; titre 15px/600 ; méta mono 12.5px `--faint`
  `margin-top: 5px` (`durée · poids · domaine`)
- Actions `justify-content: flex-end`, `gap: 8px`, `margin-top: 18px`, 13.5px :
  « Annuler » (outline) et « Télécharger et mettre en file » (accent, 600)

---

## 11. Écrans 05 et 06

### 05 — Premier lancement (660 × 500)
Contenu centré, `max-width: 440px`, `text-align: center`
- Icône 54×54 `border-radius: 18px` `margin: 0 auto 18px` `background: var(--accentSoft)`
  `border: 1px solid var(--accent)` `color: var(--accent)` `◐` 22px
- Titre 22px/600/-.02em · description 14.5px `--dim` `margin-top: 8px`
- Barre : hauteur **5 px**, `border-radius: 999px`, `background: var(--sunken)`,
  remplissage `--accent`, `margin: 22px 0 10px`
- Ligne d'état `space-between`, mono 12.5px `--faint` : `307 / 809 Mo` · `18 Mo/s · 28 s`
- « Annuler » 13.5px `--dim` `margin-top: 22px`

### 06 — États (620 × 500)
`padding: 26px`, `gap: 14px`
- **État vide** : `flex: 1`, `border: 1.5px dashed var(--dashed)`, `border-radius: 18px`,
  `background: var(--dropBg)`, `blur(18px)`, centré —
  titre 19px/600 « Rien encore », corps 14px `--dim` `margin-top: 8px` `max-width: 380px`
- **Cartes d'état** : `border-radius: 14px`, `padding: 14px 16px`, `blur(18px)`,
  titre 14.5px/600, corps 13.5px `--dim` `margin-top: 3px`
  - neutre : `border: 1px solid var(--border)`
  - erreur : `border: 1px solid var(--errBorder)`, titre en `--err`
  - liens inline en `--accent` ; lien discret en `border-bottom: 1px solid var(--dim)`

---

## 12. Onglet « Apparence » — extension du design

> **Cet onglet n'existe pas dans le prototype.** Il est demandé explicitement par l'utilisateur.
> Il ne doit **rien inventer** : tous ses composants sont repris à l'identique du spécimen
> d'en-tête du prototype (`Sillage.dc.html` lignes 32–58), replacés dans le châssis de l'écran 03.

Position : **5ᵉ onglet**, après « Système ».

Contenu, dans une carte `background: var(--panel)`, `border: 1px solid var(--border)`,
`border-radius: 16px`, `blur(20px)`, `padding: 20px 22px`, `display: flex`, `gap: 26px`,
`align-items: flex-start` :

**Colonne gauche** (`flex-direction: column; gap: 10px; align-items: center`)
- Canvas de la roue : 320×320 internes, 160×160 affichés, `border-radius: 50%`,
  `cursor: crosshair`, `display: block` — formule du §3
- Légende « roue d'accent » mono 12px `--dim`

**Colonne droite** (`flex-direction: column; gap: 16px; padding-top: 4px`)
1. Libellé « Accent » 11px `letter-spacing: .16em` `uppercase` `--dim` `margin-bottom: 8px`
   - Pastille 30×30 `border-radius: 9px` `background: var(--accent)`,
     `box-shadow: 0 0 0 1px var(--border), 0 6px 18px -6px var(--accent)`
   - Hex en mono 14px, `gap: 10px`, en majuscules (`#E08A4B`)
2. Rangée de 5 préréglages (§2.5) : boutons 24×24 `border-radius: 8px`
   `border: 1px solid var(--border)`, `gap: 8px`, `cursor: pointer`,
   survol `transform: translateY(-2px)`
3. Libellé « Thème » (même style que « Accent »)
   - Segmenté : `background: var(--sunken)`, `border-radius: 10px`, `padding: 3px`, `gap: 6px`
   - Boutons `padding: 7px 14px`, `border-radius: 8px`, Figtree 13px, `color: var(--text)`
   - Sélectionné : `background: var(--sel)` · non sélectionné : `transparent`
   - Libellés : « Sombre », « Clair »

**Comportement** — application immédiate, sans rechargement ni bouton de validation.
Un changement d'accent doit re-teinter : `--accent`, `--accentSoft`, `--dashed`, `--onAccent`,
le fond maillé, les barres lues de la forme d'onde, le vumètre, toutes les bordures et
libellés accentués. Persisté immédiatement dans les réglages.

**Champ hex éditable** : le prototype affiche le hex en lecture seule. Le rendre saisissable
(validation `^#[0-9A-Fa-f]{6}$`, normalisation en majuscules) est une extension autorisée et
souhaitable, sans changer la métrique du composant.

---

## 13. Inventaire des états à implémenter

| Écran | États |
|---|---|
| Bibliothèque | vide · en cours (streaming) · en file · en échec · aucun résultat · filtre actif sans résultat · téléchargement de modèle en cours |
| Détail | transcription en cours · terminée · LLM en cours · LLM obsolète · LLM désactivé · LLM en erreur · édité · lecture en cours |
| Réglages | 5 onglets · avertissement VRAM · vocabulaire vide/rempli |
| Modales | enregistrement (prêt · en cours · pause) · URL (saisie · résolution · résolu · échec) |
| Premier lancement | téléchargement · échec réseau · annulé |
| Global | thème sombre · thème clair · chaque accent |

**Chaque état de ce tableau doit exister dans les deux thèmes.**

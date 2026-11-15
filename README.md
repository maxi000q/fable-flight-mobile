Open world flight simulator in pure rust created by Claude Fable 5.2 and Opus 5.5

# Demo
[Demo](https://github.com/user-attachments/assets/4f8833da-13c0-4d20-a50b-c0acf2d88df1)



# Compiling
* Install rust
```
cargo build --release
```

running
```
cargo run --release
```

# Language
The interface is in English or French. It follows the system language, can be changed in Settings (start screen or pause menu), and is remembered. For one run: `cargo run --release -- --lang fr`.

Settings (language, sound, camera, HUD, anti-aliasing) are kept in `settings.json` in the config folder (`~/.config/fablegame` on Linux).

# Contributions
Please contribute anything if you want to! Improvements to flight sim realism, graphics, optimization, UI, and especially plane models are needed! (((=

# Français
Simulateur de vol en monde ouvert, en Rust pur, créé par Claude Fable 5.2 et Opus 5.5.

## Compiler et lancer
Installez Rust (https://rustup.rs), puis :
```
cargo run --release
```
Sous Linux, il faut aussi les bibliothèques ALSA et udev (`libasound2-dev libudev-dev` sur Debian/Ubuntu).

## Langue
L'interface est disponible en anglais et en français. Elle suit la langue du système, se change dans les Réglages (écran de départ ou menu pause) et est mémorisée. Pour une seule session : `cargo run --release -- --lang fr`.

Les réglages (langue, son, caméra, HUD, anticrénelage) sont enregistrés dans `settings.json`, dans le dossier de configuration (`~/.config/fablegame` sous Linux).

## Commandes principales
- **W / S** : piquer / cabrer, **A / D** : roulis, **Q / E** : palonnier
- **Maj / Ctrl** : gaz, **F / V** : volets, **L** : train d'atterrissage
- **Tab** : panneau de vol, **Échap** : menu pause, **F1** : aide-mémoire des touches

## Contributions
Toute contribution est bienvenue : réalisme du vol, graphismes, optimisation, interface et surtout nouveaux modèles d'avions. Pour ajouter une langue, voir `src/i18n.rs`.

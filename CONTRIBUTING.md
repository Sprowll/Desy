# Contribuer à Desy

Merci de ton intérêt ! Quelques règles pour que tout se passe bien.

## Avant de coder

- Pour un bug : ouvre une issue avec les étapes pour le reproduire, ton système et la version de Desy.
- Pour une nouvelle fonctionnalité : ouvre d'abord une issue pour en discuter, afin d'éviter un travail qui ne serait pas intégré.

## Pull requests

1. Crée une branche depuis `main`.
2. Garde la PR ciblée sur un seul sujet.
3. Vérifie que tout passe en local :
   ```sh
   cargo fmt --all
   cargo clippy --workspace --all-targets -- -D warnings
   cargo test --workspace
   cargo deny --all-features check
   ```
4. Ajoute des tests pour la logique (`desy-core`, `editor.rs`).
5. Messages de commit au format [Conventional Commits](https://www.conventionalcommits.org/fr/) (`feat:`, `fix:`, `docs:`, `chore:`…) : ils servent à rédiger les notes de version.

Les nouvelles dépendances doivent avoir une licence permissive acceptée par `deny.toml`.

## Licence des contributions

Desy est distribué sous [PolyForm Noncommercial 1.0.0](LICENSE.md), et l'auteur peut proposer des licences commerciales séparées.

En envoyant une contribution (pull request, patch, suggestion de code), tu certifies que tu en es l'auteur ou que tu as le droit de la soumettre, et tu accordes à Redginald Godeau une licence perpétuelle, mondiale, gratuite et irrévocable pour utiliser, modifier, distribuer et sous-licencier ta contribution, y compris sous d'autres licences (commerciales ou non). Tu restes propriétaire de ta contribution.

Si tu n'es pas d'accord avec ces conditions, n'envoie pas de contribution.

## Code de conduite

Sois respectueux et constructif. Les comportements hostiles, discriminatoires ou le harcèlement entraînent l'exclusion du projet.

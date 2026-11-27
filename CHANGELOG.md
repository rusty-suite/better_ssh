# Changelog

Toutes les modifications notables de ce projet sont documentées ici.

Le format suit [Keep a Changelog](https://keepachangelog.com/fr/1.1.0/)
et le projet respecte le [Versionnage sémantique](https://semver.org/lang/fr/) :
`MAJEUR.MINEUR.CORRECTIF`, tags Git `vX.Y.Z` identiques à la version de `Cargo.toml`.

## [Non publié]

### Modifié
- Licence : GNU GPL v3 ou ultérieure (`GPL-3.0-or-later`) à la place de PolyForm Noncommercial 1.0.0. Le programme devient un logiciel libre, utilisable par tous.

## [0.2.0] - 2026-11-27

### Ajouté
- Interface entièrement traduite dans les 7 langues (EN, FR, DE, IT, CH-fr, CH-de, CH-it) :
  barre de statut, préférences, à propos, scanner réseau, snippets, moniteur système,
  terminal, dialogues Telnet et de connexion depuis le scan.
- Messages de session (connexion, fin, erreurs, opérations SFTP) traduits.
- Tests garantissant que toutes les langues ont les mêmes clés.
- Binaire Linux x86_64 publié avec la release.

### Modifié
- Un fichier de langue téléchargé ou local incomplet est complété par la version
  intégrée de sa langue, puis par l'anglais.
- Tailles de fichier affichées avec les unités de la langue active.
- Licence déclarée : PolyForm Noncommercial 1.0.0 (comme le fichier de licence du dépôt).

### Corrigé
- Fonctionnalités perdues lors d'un merge (mai 2026) rétablies : explorateur SFTP complet
  (navigation, menu contextuel, renommage, vue grille / liste), vault chiffré (adresse,
  utilisateur, historique, vérification de la clé maître), téléchargement et choix des
  langues, historique du terminal.
- La fenêtre de profil perdait le focus à la première lettre saisie dans « Nom ».
- Dates de l'explorateur SFTP fausses (années bissextiles et longueurs de mois ignorées).
- Le titre de la fenêtre de langue restait « Language / Langue ».

## [0.1.0] - 2026-04-20

Première pré-release publique (tag historique `Win_&_Linux`).

[Non publié]: https://github.com/rusty-suite/better_ssh/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/rusty-suite/better_ssh/compare/Win_%26_Linux...v0.2.0
[0.1.0]: https://github.com/rusty-suite/better_ssh/releases/tag/Win_%26_Linux

# Politique de sécurité

## Versions prises en charge

Seule la dernière version publiée sur la page [Releases](https://github.com/Sprowll/Desy/releases/latest) reçoit des correctifs de sécurité.

## Signaler une vulnérabilité

**N'ouvre pas d'issue publique.** Utilise le signalement privé de GitHub :
onglet **Security** du dépôt → **Report a vulnerability**.

Indique la version de Desy, ton système, les étapes pour reproduire le problème et son impact (par exemple un fichier `.desy`, une palette ou une image piégés). Tu recevras une réponse sous 7 jours.

## Vérifier un téléchargement

Chaque release contient `SHA256SUMS.txt` et une attestation de provenance signée :

```sh
gh attestation verify <fichier> --repo Sprowll/Desy
```

Ne télécharge Desy que depuis la page Releases de ce dépôt.

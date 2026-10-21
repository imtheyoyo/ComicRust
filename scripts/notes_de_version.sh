#!/usr/bin/env bash
# Extrait de CHANGELOG.md la section d'une version (« ## [vX.Y.Z] … » jusqu'à la suivante),
# avec des liens absolus vers le dépôt : ce sont les notes de la release GitHub.
# Usage : scripts/notes_de_version.sh v0.4.1 > notes.md   (échoue si la section manque)
set -euo pipefail
tag="$1"
repo="${GITHUB_REPOSITORY:-imtheyoyo/ComicRust}"
notes=$(awk -v t="## [$tag]" '
  index($0, t) == 1 { on = 1; next }
  on && /^## \[/ { exit }
  on { print }
' CHANGELOG.md | sed -e "s#](recette/#](https://github.com/$repo/blob/main/recette/#g" \
                     -e "s#](docs/#](https://github.com/$repo/blob/main/docs/#g")
if [ -z "$(echo "$notes" | tr -d '[:space:]')" ]; then
  echo "::error::CHANGELOG.md n'a pas de section « ## [$tag] »" >&2
  exit 1
fi
printf '%s\n' "$notes" | sed -e '/./,$!d'

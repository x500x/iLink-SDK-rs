#!/usr/bin/env bash
# Configure the GitHub Actions secrets the release workflow needs.
#
# Run it from anywhere; it uses the `gh` CLI (already authenticated as x500x).
# Press Enter to skip any value you do not have yet — the release workflow
# degrades gracefully when a secret is missing.
#
#   bash bindings/scripts/set_secrets.sh
#
# Where to get each value (all browser-based, one time):
#   PyPI token        https://pypi.org/manage/account/token/       (scope: project "ilink-sdk")
#   crates.io token   https://crates.io/settings/tokens            (scopes: publish-new, publish-update)
#   Sonatype user     https://central.sonatype.com/usertoken       (Generate User Token -> username/password)
set -euo pipefail

REPO="x500x/iLink-SDK-rs"

say()  { printf '\n\033[1m%s\033[0m\n' "$*"; }
note() { printf '  %s\n' "$*"; }

command -v gh >/dev/null || { echo "gh CLI not found"; exit 1; }

say "Configuring secrets for $REPO"
note "Press Enter to skip a value."

say "1/4  PyPI API token"
note "Create one at https://pypi.org/manage/account/token/"
note "  Scope: 'Project: ilink-sdk' (or 'Entire account' for the very first upload)"
read -rsp "  Paste token (pypi-...): " PYPI_TOKEN; echo
if [ -n "${PYPI_TOKEN:-}" ]; then
  printf '%s' "$PYPI_TOKEN" | gh secret set PYPI_API_TOKEN --repo "$REPO"
  echo "  ✓ PYPI_API_TOKEN set"
else
  echo "  - skipped"
fi

say "2/4  crates.io API token"
note "Create one at https://crates.io/settings/tokens"
note "  Scopes: publish-new, publish-update"
read -rsp "  Paste token: " CARGO_TOKEN; echo
if [ -n "${CARGO_TOKEN:-}" ]; then
  printf '%s' "$CARGO_TOKEN" | gh secret set CARGO_REGISTRY_TOKEN --repo "$REPO"
  echo "  ✓ CARGO_REGISTRY_TOKEN set"
else
  echo "  - skipped"
fi

say "3/4  Sonatype Central Portal username"
note "Generate at https://central.sonatype.com/usertoken"
read -rp "  Username: " MAVEN_USER
if [ -n "${MAVEN_USER:-}" ]; then
  printf '%s' "$MAVEN_USER" | gh secret set MAVEN_CENTRAL_USERNAME --repo "$REPO"
  echo "  ✓ MAVEN_CENTRAL_USERNAME set"
else
  echo "  - skipped"
fi

say "4/4  Sonatype Central Portal password"
note "The token shown next to the username above."
read -rsp "  Password: " MAVEN_PASS; echo
if [ -n "${MAVEN_PASS:-}" ]; then
  printf '%s' "$MAVEN_PASS" | gh secret set MAVEN_CENTRAL_PASSWORD --repo "$REPO"
  echo "  ✓ MAVEN_CENTRAL_PASSWORD set"
else
  echo "  - skipped"
fi

say "Done. Secrets now configured:"
gh secret list --repo "$REPO"

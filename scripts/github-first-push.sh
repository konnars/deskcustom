#!/bin/bash
# First-time push to GitHub. Run from repo root.
set -euo pipefail

REPO_NAME="${1:-deskcustom}"

if ! command -v git >/dev/null; then
  echo "git not found"
  exit 1
fi

cd "$(dirname "$0")/.."

if [ ! -d .git ]; then
  git init
  git branch -M main
fi

git add -A
git status

echo ""
echo "Commit? (creates initial commit if needed)"
read -r -p "Message [Initial commit]: " MSG
MSG="${MSG:-Initial commit}"
git commit -m "$MSG" || true

echo ""
echo "Create repo on GitHub: https://github.com/new  name=$REPO_NAME"
echo "Then run:"
echo "  git remote add origin git@github.com:YOUR_USER/$REPO_NAME.git"
echo "  git push -u origin main"
echo ""
echo "Build Windows installer:"
echo "  GitHub → Actions → Build Windows Installer → Run workflow"
echo "  OR tag release:"
echo "  git tag v0.1.0 && git push origin v0.1.0"

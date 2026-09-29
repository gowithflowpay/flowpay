#!/usr/bin/env bash
set -euo pipefail
export FILTER_BRANCH_SQUELCH_WARNING=1
git filter-branch -f \
  --env-filter '
if [ "$GIT_AUTHOR_EMAIL" = "mrteesolution88@gmail.com" ]; then
  GIT_AUTHOR_NAME="Mrteesoft"
  GIT_AUTHOR_EMAIL="mrteesoft@gmail.com"
fi
if [ "$GIT_COMMITTER_EMAIL" = "mrteesolution88@gmail.com" ]; then
  GIT_COMMITTER_NAME="Mrteesoft"
  GIT_COMMITTER_EMAIL="mrteesoft@gmail.com"
fi
export GIT_AUTHOR_NAME GIT_AUTHOR_EMAIL GIT_COMMITTER_NAME GIT_COMMITTER_EMAIL
' \
  --msg-filter 'sed "/Codebuff/d"' \
  -- main

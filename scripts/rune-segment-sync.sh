#!/usr/bin/env bash
# Segment <-> parent sync helper for multi-machine Rune migration.
#
#   sync-from-parent   (segment machines)  pull latest parent into your segment
#   merge-to-parent    (coordinator)       merge one/all segment(s) into parent
#   refresh-segments   (coordinator)       fast-forward all segments to parent
#
# Parent: feat/rune-native-ui-migration
# Segments: feat/rune-native-ui-migration-segment-{1..4}
set -uo pipefail
cd "$(dirname "$0")/.." || exit 1
PARENT=feat/rune-native-ui-migration
SEG="$PARENT-segment-"
ACTION=${1:?usage: rune-segment-sync.sh sync-from-parent|merge-to-parent|refresh-segments [N]}

die() { echo "✖ $*" >&2; exit 1; }
note() { echo "▶ $*"; }

case "$ACTION" in
  sync-from-parent)
    # A segment machine calls this. Assumes current branch is its own segment.
    cur=$(git branch --show-current)
    [[ "$cur" == "$SEG"* ]] || die "current branch '$cur' is not a segment branch"
    git fetch origin "$PARENT" || die "fetch parent failed"
    note "rebasing $cur onto origin/$PARENT"
    git rebase "origin/$PARENT" || die "rebase conflicted — resolve, then 'git rebase --continue', then push --force-with-lease"
    git push --force-with-lease origin "$cur" || die "push failed"
    note "$cur is now up to date with parent"
    ;;
  merge-to-parent)
    # Coordinator calls this. Merges segment N (or all 1..4) into parent.
    cur=$(git branch --show-current)
    [ "$cur" = "$PARENT" ] || die "checkout $PARENT first (currently on $cur)"
    git fetch origin || die "fetch failed"
    if [ "${2:-all}" = all ]; then ns="1 2 3 4"; else ns="${2:?give segment number}"; fi
    for n in $ns; do
      br="$SEG$n"
      git fetch origin "$br" 2>/dev/null || { note "seg$n: branch not on remote, skip"; continue; }
      ahead=$(git rev-list --count "HEAD..origin/$br")
      if [ "$ahead" -eq 0 ]; then note "seg$n: 0 commits ahead, skip"; continue; fi
      note "merging origin/$br ($ahead commits) into $PARENT"
      git merge --no-ff "origin/$br" -m "feat(rune-segment): merge segment $n" \
        || die "merge conflict in seg$n — resolve and commit before continuing"
      git push origin "$PARENT" || die "push parent failed"
    done
    ;;
  refresh-segments)
    # Coordinator: after parent advanced, fast-forward each segment to parent
    # so machines starting fresh see the latest bases (only if a segment has 0
    # unique commits; never clobber a machine's in-flight work).
    cur=$(git branch --show-current)
    [ "$cur" = "$PARENT" ] || die "checkout $PARENT first"
    git fetch origin "$PARENT" || die "fetch failed"
    for n in 1 2 3 4; do
      br="$SEG$n"
      git fetch origin "$br" 2>/dev/null || { note "seg$n: not on remote, skip"; continue; }
      ahead=$(git rev-list --count "origin/$PARENT..origin/$br")
      if [ "$ahead" -ne 0 ]; then
        note "seg$n has $ahead unique commit(s) — NOT fast-forwarding (would clobber its work)"
        continue
      fi
      # local: force the local segment branch to parent and push
      git branch -f "$br" "origin/$PARENT"
      git push --force-with-lease origin "$br" || die "push seg$n failed"
      note "seg$n fast-forwarded to $(git rev-parse --short origin/$PARENT)"
    done
    ;;
  *) die "unknown action '$ACTION'" ;;
esac

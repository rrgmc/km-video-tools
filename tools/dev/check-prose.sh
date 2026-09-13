#!/usr/bin/env bash
#
# Prose here states the rule, not how the rule was arrived at.
#
#   tools/dev/check-prose.sh --changed   # only the lines this branch adds -- run this before a merge
#   tools/dev/check-prose.sh             # every tracked file, which is a worklist
#   tools/dev/check-prose.sh --list      # ...and print the shapes it looks for, then check
#
# The standing decision is `How a document in this repository is written` in docs/decisions.md, and
# the short form is rule 1 in CLAUDE.md. This keeps the mechanical half of it true. The half it
# cannot see is the important one -- a paragraph of reassurance has no distinctive shape, and neither
# does an appositive tail -- so a clean run is a floor rather than a pass.
#
# **`--changed` is the mode to wire into anything**, and the whole-tree mode is a worklist rather
# than a gate: it holds what a branch *adds* to the standard, which is what keeps the count falling
# instead of rising. It reads added lines by line and not by file, so touching a file does not
# inherit that file's backlog -- a gate that did would teach people to leave files alone.
#
# **What it matches is the list that decision gives, in the order it gives them**: what something
# used to be, chronology, and meta-commentary on the writing. Those three have shapes; the other two
# do not. It runs over tracked files only, and over code comments as well as documents, because the
# decision applies the rule to both.
#
# `task lint:prose` is the `--changed` form. It is deliberately not part of `task check`.

# **No `-e`**, unlike the other script in this folder: a grep that matches nothing exits 1, and here
# that is the ordinary case rather than a failure.
set -uo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/../.."
[ -f Cargo.toml ] && [ -d crates ] || { echo "${0##*/}: not at the repository root -- landed in $PWD" >&2; exit 2; }

self='check-prose'

# -- The shapes -----------------------------------------------------------------------------------

# One extended-regex per shape, so a hit can name the rule it broke rather than a pattern number.
#
# `\b` around the short ones: "used to" has to catch "it used to be" and must not catch
# "accustomed to". The chronology shapes are deliberately narrow -- a *measurement* may be dated --
# so only a version-numbered or milestone-numbered claim is matched.
#
# **A bare `stated rather than` is not one of them**, though the decision's own example of
# meta-commentary is `recorded rather than glossed`. Here `stated rather than guessed` is the house
# phrasing for a rule about the tool -- whether a URL means one video or a playlist is stated rather
# than guessed -- and a pattern flagging that would be the checker deciding the prose. What is
# matched below is only the forms whose subject is the writing.
#
# **`worth knowing` is out for the same reason.** `Taskfile.yml` uses it of a flag -- the one flag
# worth knowing -- where the subject is the flag and the sentence is plain usefulness. What the
# decision names is `worth saying out loud`, whose subject is the writing, and `recording`, `saying`
# and `reading` reach that without reaching the other.
SHAPES=(
  "what something used to be|\\b(used to (be|say|sit|carry|live|exist|read|have|do|call|spell|hold|mean))\\b"
  "what something used to be|\\bno longer\\b"
  "what something used to be|\\b(this|which) (reverses|replaces a|used to)\\b"
  "what something used to be|\\bwas (billed|formerly|previously)\\b"
  "what something used to be|\\bformer(ly)? (name|default|behaviou?r|spelling)\\b"
  "chronology|\\b(since|until|before|after|in) [0-9]+\\.[0-9]+(\\.[0-9a-z]+)?\\b"
  "chronology|\\bfor (two|three|four|five|several) milestones\\b"
  "chronology|\\bmilestone [0-9]"
  "meta-commentary on the writing|\\bworth (recording|saying|reading)\\b"
  "meta-commentary on the writing|\\b(recorded|noted) rather than\\b"
  "meta-commentary on the writing|\\bstated rather than (glossed|narrated)\\b"
  "meta-commentary on the writing|\\bthat is the record of\\b"
  "meta-commentary on the writing|\\bthis (paragraph|sentence|entry|row) (replaces|used to)\\b"
  "meta-commentary on the writing|\\brather than an (accident|oversight|omission)\\b"
)

# Three paths, for three different reasons. This script's own header names every shape it hunts. And
# `htmx.min.js` is vendored and is one 50 KB line, so a single match in it prints the whole file to
# the terminal -- a size exemption rather than licence to write badly in it.
#
# `CHANGELOG.md` is the third, and it is the only one exempt for what it says rather than for what it
# is. The shapes above are that document's own vocabulary: an entry exists to say what a release
# changed, which cannot be written without naming the state before it. See
# `The changelog records what changed, and every other document states what is` in docs/decisions.md.
#
# Nothing else is exempt, `docs/decisions.md` included: that file is where somebody writes up a
# decision they have just made, which is where this rule is most easily broken.
EXEMPT='^(tools/dev/check-prose\.sh|crates/km-video-downloader/static/htmx\.min\.js|CHANGELOG\.md)$'

# Text this repository writes: documents, and the languages whose comments carry reasoning. `.xml`
# and `.iss` are in because the macOS `Distribution` and the Inno Setup script each argue a version
# floor in a comment. `.txt` and `.svg` are out because the only tracked ones are a vendored licence
# and generated path data, and `icon/` and `dist/` hold no prose at all.
KINDS=('*.md' '*.rs' '*.html' '*.css' '*.js' '*.toml' '*.sh' '*.yml' '*.xml' '*.iss')

# -- What to read ---------------------------------------------------------------------------------

CHANGED=0
for arg in "$@"; do
  [ "$arg" = "--changed" ] && CHANGED=1
done

base=""
if [ "$CHANGED" -eq 1 ]; then
  # What this branch touched, against the default branch's tip. A file added and not yet committed
  # counts, `--changed` being what somebody runs before asking for a merge.
  base=$(git merge-base origin/master HEAD 2>/dev/null || true)

  # **A missing merge base exits 2 rather than reporting clean.** A shallow checkout has no
  # `origin/master` to measure against -- a CI clone is depth 1 by default -- and a check answering
  # "nothing to read" there is a gate that passes because it looked at nothing.
  if [ -z "$base" ]; then
    printf '%s: no merge base with origin/master, so there is nothing to measure this branch against.\n' "$self" >&2
    printf '%s: a shallow clone is the usual cause -- git fetch --unshallow, or run the whole-tree form.\n' "$self" >&2
    exit 2
  fi

  mapfile -t FILES < <({
    git diff --name-only --diff-filter=d "$base" -- "${KINDS[@]}"
    git ls-files --others --exclude-standard -- "${KINDS[@]}"
  } | sort -u | grep -Ev "$EXEMPT")
else
  mapfile -t FILES < <(git ls-files "${KINDS[@]}" | grep -Ev "$EXEMPT")
fi

if [ "${1:-}" = "--list" ]; then
  echo "$self: the shapes it looks for"
  for entry in "${SHAPES[@]}"; do
    printf '  %-34s %s\n' "${entry%%|*}" "${entry#*|}"
  done
  echo
fi

if [ "${#FILES[@]}" -eq 0 ]; then
  echo "$self: nothing to read"
  exit 0
fi

# In `--changed` mode only the lines this branch *added* are read.
added_lines() {
  git diff --unified=0 "$base" -- "$1" 2>/dev/null |
    awk '/^@@/ { split($3, a, ","); start = a[1] + 0; count = (a[2] == "" ? 1 : a[2] + 0);
                 for (i = 0; i < count; i++) print start + i }'
  # An untracked file is new whole.
  if git ls-files --others --exclude-standard --error-unmatch -- "$1" >/dev/null 2>&1; then
    awk '{ print NR }' "$1"
  fi
}

# -- Reading it -----------------------------------------------------------------------------------

found=0
for file in "${FILES[@]}"; do
  scope=""
  if [ "$CHANGED" -eq 1 ]; then
    scope=$(added_lines "$file" | sort -un | tr '\n' ' ')
    [ -z "${scope// /}" ] && continue
    scope=" $scope"
  fi
  for entry in "${SHAPES[@]}"; do
    rule="${entry%%|*}"
    pattern="${entry#*|}"
    while IFS= read -r hit; do
      [ -z "$hit" ] && continue
      number="${hit%%:*}"
      if [ "$CHANGED" -eq 1 ] && [[ "$scope" != *" $number "* ]]; then
        continue
      fi
      # **A phrase inside quotation marks or a code span is being named, not used.** The decision
      # this enforces has to quote the shapes it forbids, and so does anything pointing at it, so a
      # hit surviving only inside "..." or a backtick span is a mention. Strip both and re-test:
      # what is left is the line's own voice. Without this, the first thing this fails is the rule.
      #
      # Two steps rather than one pipeline into `grep -q`: under `pipefail` a `grep -q` that exits
      # the moment it matches can leave `sed` dead of SIGPIPE, and the pipeline's non-zero status
      # would then read as a mention for a line that is a hit.
      spoken=$(printf '%s' "${hit#*:}" | sed -e 's/"[^"]*"//g' -e 's/`[^`]*`//g')
      if ! grep -qiE "$pattern" <<<"$spoken"; then
        continue
      fi
      printf '%s:%s\n    ^ %s\n' "$file" "$hit" "$rule"
      found=1
    done < <(grep -niE "$pattern" -- "$file" 2>/dev/null)
  done
done

if [ "$found" -ne 0 ]; then
  cat >&2 <<'WHY'

check-prose: the lines above narrate how a rule was arrived at rather than stating it.

  `How a document in this repository is written`, docs/decisions.md

The keep-test: would a reader who deleted the sentence either re-derive a wrong answer, or break
something silently? A sentence about a state that is gone fails it. This applies to code comments on
the same test -- why a line is the way it is earns its place; what it once said does not.
WHY
  exit 1
fi

echo "$self: clean"

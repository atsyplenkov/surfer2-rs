#!/usr/bin/env bash
# Read the four source checkouts and write sources/revisions.bundle.
# Does not commit, stage, or edit those checkouts.
set -eu

DIR=$(cd "$(dirname "$0")" && pwd)
MAIN=/home/ats/Development/surfer2-rs
ARM_A=/home/ats/.grok/worktrees/development-surfer2-rs/subagent-01a1d91d-ffd1-7df2-a99d-28189dfa59ae
ARM_B=/home/ats/.grok/worktrees/development-surfer2-rs/subagent-01a1d91d-ffd1-7df2-a99d-282f7bdcc9ba
CONT=/home/ats/.grok/worktrees/development-surfer2-rs/subagent-01a1d970-f5c4-7cc3-8bfd-7ec922c43176

CONTROL=d1a89fd66795f537b034f220ed4f838bd0fdea3b
ARM_A_COMMIT=1b6ed8ea9af51ed9f82f99d56306abc6f6359c7c
ARM_B_BASE=7d3faf62ec7bd15f46365053922de2885dac87ca

REPO=$(mktemp -d)
trap 'rm -rf "$REPO"' EXIT

git init -q "$REPO"
git -C "$REPO" fetch -q "$MAIN" "$CONTROL:refs/heads/evidence/control" "$ARM_A_COMMIT:refs/heads/evidence/arm-a"
git -C "$REPO" fetch -q "$ARM_B" "$ARM_B_BASE:refs/heads/evidence/arm-b-base"

snapshot() {
    local src="$1" dest="$2"
    mkdir -p "$dest"
    rsync -a --delete \
        --exclude .git \
        --exclude target \
        --exclude .scratch \
        --exclude evidence \
        "$src/" "$dest/"
}

git -C "$REPO" worktree add --detach "$REPO-armb" evidence/arm-b-base
snapshot "$ARM_B" "$REPO-armb"
git -C "$REPO-armb" add -A
git -C "$REPO-armb" commit -q -m "evidence: arm b dirty tree"
git -C "$REPO" branch -f evidence/arm-b-dirty "$(git -C "$REPO-armb" rev-parse HEAD)"

git -C "$REPO" worktree add --detach "$REPO-cont" evidence/arm-a
snapshot "$CONT" "$REPO-cont"
git -C "$REPO-cont" add -A
git -C "$REPO-cont" commit -q -m "evidence: continuation dirty tree"
git -C "$REPO" branch -f evidence/continuation-dirty "$(git -C "$REPO-cont" rev-parse HEAD)"

git -C "$REPO" worktree add --detach "$REPO-control-harness" evidence/control
git -C "$REPO-control-harness" checkout -q evidence/arm-a -- tests/projected_polygons.rs fixtures/projected
git -C "$REPO-control-harness" add -A
git -C "$REPO-control-harness" commit -q -m "evidence: arm a harness on control d1a89fd"
git -C "$REPO" branch -f evidence/control-harness "$(git -C "$REPO-control-harness" rev-parse HEAD)"

git -C "$REPO" worktree add --detach "$REPO-armb-harness" evidence/arm-b-dirty
git -C "$REPO-armb-harness" checkout -q evidence/arm-a -- tests/projected_polygons.rs fixtures/projected
git -C "$REPO-armb-harness" add -A
git -C "$REPO-armb-harness" commit -q -m "evidence: arm a harness on arm b dirty engine"
git -C "$REPO" branch -f evidence/arm-b-arm-a-harness "$(git -C "$REPO-armb-harness" rev-parse HEAD)"

git -C "$REPO" symbolic-ref HEAD refs/heads/evidence/control
git -C "$REPO" bundle create "$DIR/revisions.bundle" HEAD --branches
git bundle verify "$DIR/revisions.bundle"

echo "BUNDLE $DIR/revisions.bundle"
git -C "$REPO" for-each-ref --format='%(refname) %(objectname) %(tree)' refs/heads/evidence

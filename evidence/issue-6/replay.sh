#!/usr/bin/env bash
set -eu

DIR=$(cd "$(dirname "$0")" && pwd)
MANIFEST="$DIR/manifest.json"
LOGS="$DIR/logs"
FRAG="$DIR/fragments"
LOCK="$DIR/.replay.lock"
CASE_TIMEOUT_S=${CASE_TIMEOUT_S:-600}
BUILD_TIMEOUT_S=${BUILD_TIMEOUT_S:-900}

mkdir -p "$LOGS" "$FRAG"
exec 9>"$LOCK"
trap 'rm -f "$LOCK"' EXIT
flock 9
rm -rf "$FRAG"
mkdir -p "$FRAG"

plan_tsv() {
    python3 - "$MANIFEST" << 'PY'
import json, sys
manifest = json.load(open(sys.argv[1]))
for rev in manifest["revisions"]:
    fields = [
        rev["id"],
        rev["checkout"],
        rev["commit"],
        "true" if rev["captured_diff"]["includes_untracked"] else "false",
        rev["captured_diff"]["path"],
        "true" if rev["run_tests"] else "false",
        rev.get("skip_reason") or "",
    ]
    print("\t".join(fields))
PY
}

capture_now() {
    local repo="$1" include="$2" dest="$3"
    : > "$dest.tracked"
    : > "$dest.cached"
    : > "$dest.untracked"
    git -C "$repo" diff HEAD > "$dest.tracked"
    git -C "$repo" diff --cached > "$dest.cached"
    if [ "$include" = true ]; then
        git -C "$repo" ls-files --others --exclude-standard | sort | while IFS= read -r f; do
            [ -n "$f" ] || continue
            case "$f" in
                src/*|tests/*|docs/*|fixtures/*) ;;
                *) continue ;;
            esac
            git -C "$repo" diff --no-index -- /dev/null "$f" >> "$dest.untracked" || true
        done
    fi
    cat "$dest.tracked" "$dest.cached" "$dest.untracked" > "$dest"
}

check_revision() {
    local id="$1" checkout="$2" expect="$3" include="$4" diffrel="$5" run_tests="$6" skip_reason="$7"
    local frag="$FRAG/$id.json"
    local diffabs="$DIR/$diffrel"
    local head porcelain
    head=$(git -C "$checkout" rev-parse HEAD)
    porcelain=$(git -C "$checkout" status --porcelain)
    local tmp
    tmp=$(mktemp -d)
    capture_now "$checkout" "$include" "$tmp/now.diff"
    local source_ok=yes source_note=""
    if [ "$head" != "$expect" ]; then
        source_ok=no
        source_note="HEAD is $head, expected $expect"
    elif ! cmp -s "$tmp/now.diff" "$diffabs"; then
        source_ok=no
        source_note="captured diff no longer matches the checkout"
    fi
    rm -rf "$tmp"

    python3 - "$frag" "$id" "$head" "$porcelain" "$source_ok" "$source_note" "$run_tests" "$skip_reason" "$checkout" << 'PY'
import json, sys
frag, rid, head, porcelain, source_ok, source_note, run_tests, skip_reason, checkout = sys.argv[1:]
open(frag, "w").write(json.dumps({
    "id": rid,
    "head": head,
    "porcelain": porcelain,
    "source_ok": source_ok == "yes",
    "source_note": source_note,
    "run_tests": run_tests == "true",
    "skip_reason": skip_reason or None,
    "checkout": checkout,
    "profiles": {},
}))
PY

    if [ "$source_ok" != yes ]; then
        return 1
    fi
    return 0
}

run_tests() {
    local id="$1" checkout="$2" run_tests="$3"
    local frag="$FRAG/$id.json"
    if [ "$run_tests" != true ]; then
        return 0
    fi

    local profile cargo_profile
    for profile in release debug; do
        if [ "$profile" = release ]; then
            cargo_profile=(--release)
        else
            cargo_profile=()
        fi
        local build_log="$LOGS/$id-$profile-build.json"
        local build_rc exe exe_sha
        set +e
        (
            cd "$checkout" || exit 97
            CARGO_TERM_COLOR=never timeout -k 20 "$BUILD_TIMEOUT_S" \
                cargo test "${cargo_profile[@]}" --test projected_polygons --locked \
                --no-run --message-format=json
        ) > "$build_log" 2>"$LOGS/$id-$profile-build.err"
        build_rc=$?
        set -e
        exe=""
        exe_sha=""
        if [ "$build_rc" -eq 0 ]; then
            exe=$(python3 - "$build_log" << 'PY'
import json, sys
path = ""
for line in open(sys.argv[1], errors="replace"):
    line = line.strip()
    if not line.startswith("{"):
        continue
    try:
        obj = json.loads(line)
    except json.JSONDecodeError:
        continue
    if obj.get("reason") != "compiler-artifact":
        continue
    target = obj.get("target") or {}
    if target.get("name") == "projected_polygons" and obj.get("executable"):
        path = obj["executable"]
print(path)
PY
)
            if [ -n "$exe" ] && [ -f "$exe" ]; then
                exe_sha=$(sha256sum "$exe" | awk '{print $1}')
            else
                build_rc=98
            fi
        fi
        python3 - "$frag" "$profile" "$build_rc" "$exe" "$exe_sha" << 'PY'
import json, sys
frag, profile, rc, exe, exe_sha = sys.argv[1:]
doc = json.load(open(frag))
doc["profiles"][profile] = {
    "build_exit": int(rc),
    "executable": exe or None,
    "executable_sha256": exe_sha or None,
}
json.dump(doc, open(frag, "w"))
PY
        if [ "$build_rc" -ne 0 ]; then
            continue
        fi

        python3 - "$MANIFEST" "$id" << 'PY' > "$FRAG/$id-$profile.cases"
import json, sys
manifest = json.load(open(sys.argv[1]))
rid = sys.argv[2]
for rev in manifest["revisions"]:
    if rev["id"] == rid:
        for case in rev["cases"]:
            print(case["case"] + "\t" + case["test"])
PY
        while IFS=$'\t' read -r case test; do
            [ -n "$case" ] || continue
            local log="$LOGS/$id-$profile-$case.log"
            local cmd rc release_flag=""
            if [ "$profile" = release ]; then
                release_flag="--release "
            fi
            cmd="cd $checkout && CARGO_TERM_COLOR=never cargo test ${release_flag}--test projected_polygons --locked -- $test -- --exact --test-threads=1 --nocapture"
            set +e
            (
                cd "$checkout" || exit 97
                CARGO_TERM_COLOR=never timeout -k 20 "$CASE_TIMEOUT_S" \
                    cargo test "${cargo_profile[@]}" --test projected_polygons --locked -- \
                    "$test" -- --exact --test-threads=1 --nocapture
            ) > "$log" 2>&1
            rc=$?
            set -e
            python3 - "$frag" "$profile" "$case" "$test" "$rc" "$cmd" "$log" "$CASE_TIMEOUT_S" << 'PY'
import json, re, sys
frag, profile, case, test, rc, cmd, log, ceiling = sys.argv[1:]
rc = int(rc)
text = open(log, errors="replace").read()
def signal():
    if rc == 124:
        return f"{test}: timeout after {ceiling}s"
    m = re.search(r"test result: (\w+)\. ([^\n]+)", text)
    if m:
        kind, rest = m.group(1), m.group(2)
        if "0 passed; 0 failed" in rest and re.search(r"0 ignored", rest):
            return f"{test}: test name absent"
        if re.search(r"0 passed; 0 failed; [1-9]\d* ignored", rest):
            return f"{test}: ignored, not executed"
        if kind == "ok" and rc == 0:
            return f"{test}: ok"
        chunks = re.findall(
            r"panicked at [^\n]*\n(.*?)(?:\nnote: run with|\nerror: test failed|\nfailures:\n|\Z)",
            text,
            re.S,
        )
        if chunks:
            msg = chunks[-1].strip()
            msg = re.sub(r"\n+", " ", msg)
            if len(msg) > 2000:
                msg = msg[:2000] + " ...truncated"
            return f"{test}: {msg}"
        return f"{test}: FAILED without a panic message"
    for line in text.splitlines():
        if line.startswith("error:"):
            return f"{test}: {line}"
    tail = text.strip().splitlines()
    last = tail[-1] if tail else f"no output, exit {rc}"
    return f"{test}: {last}"
doc = json.load(open(frag))
doc["profiles"][profile].setdefault("cases", [])
doc["profiles"][profile]["cases"].append({
    "case": case,
    "test": test,
    "command": cmd,
    "exit_code": rc,
    "signal": signal(),
    "log": log,
})
json.dump(doc, open(frag, "w"))
PY
        done < "$FRAG/$id-$profile.cases"
    done
}

mapfile -t plan_rows < <(plan_tsv)
pids=()
for row in "${plan_rows[@]}"; do
    IFS=$'\t' read -r id checkout commit include diffrel run_tests skip_reason <<< "$row"
    [ -n "$id" ] || continue
    check_revision "$id" "$checkout" "$commit" "$include" "$diffrel" "$run_tests" "$skip_reason" &
    pids+=($!)
done

fail=0
for pid in "${pids[@]}"; do
    wait "$pid" || fail=1
done

if [ "$fail" -eq 0 ]; then
    pids=()
    for row in "${plan_rows[@]}"; do
        IFS=$'\t' read -r id checkout commit include diffrel run_tests skip_reason <<< "$row"
        [ -n "$id" ] || continue
        run_tests "$id" "$checkout" "$run_tests" &
        pids+=($!)
    done
    fail=0
    for pid in "${pids[@]}"; do
        wait "$pid" || fail=1
    done
fi

CASE_TIMEOUT_S="$CASE_TIMEOUT_S" python3 - "$MANIFEST" "$FRAG" "$fail" << 'PY'
import datetime, json, pathlib, subprocess, sys
manifest_path, frag_dir, fail = sys.argv[1:]
root = pathlib.Path(manifest_path).parent
manifest = json.loads(pathlib.Path(manifest_path).read_text())
order_rev = [r["id"] for r in manifest["revisions"]]
order_profile = ["release", "debug"]
case_index = {
    (rev["id"], case["case"]): i
    for rev in manifest["revisions"]
    for i, case in enumerate(rev["cases"])
}
observations = []
runs = []
for path in sorted(pathlib.Path(frag_dir).glob("*.json")):
    doc = json.loads(path.read_text())
    runs.append({
        "revision_id": doc["id"],
        "head": doc["head"],
        "porcelain": doc["porcelain"],
        "source_ok": doc["source_ok"],
        "source_note": doc["source_note"],
    })
    rev = next(r for r in manifest["revisions"] if r["id"] == doc["id"])
    for profile in order_profile:
        built = doc["profiles"].get(profile)
        for case in rev["cases"]:
            row = {
                "revision_id": doc["id"],
                "profile": profile,
                "case": case["case"],
                "test": case["test"],
                "checkout": doc["checkout"],
            }
            if not doc["source_ok"]:
                row.update({
                    "command": None,
                    "exit_code": None,
                    "signal": f"{case['test']}: {doc['source_note']}",
                    "executable": None,
                    "executable_sha256": None,
                    "log": None,
                    "ran": False,
                })
            elif not doc["run_tests"]:
                row.update({
                    "command": None,
                    "exit_code": None,
                    "signal": f"{case['test']}: {doc['skip_reason']}",
                    "executable": None,
                    "executable_sha256": None,
                    "log": None,
                    "ran": False,
                })
            elif not built or built["build_exit"] != 0:
                rc = None if not built else built["build_exit"]
                row.update({
                    "command": f"cd {doc['checkout']} && CARGO_TERM_COLOR=never cargo test {'--release ' if profile == 'release' else ''}--test projected_polygons --locked --no-run --message-format=json",
                    "exit_code": rc,
                    "signal": f"{case['test']}: build failed, exit {rc}",
                    "executable": None,
                    "executable_sha256": None,
                    "log": str(root / "logs" / f"{doc['id']}-{profile}-build.err"),
                    "ran": False,
                })
            else:
                hit = next((c for c in built.get("cases", []) if c["case"] == case["case"]), None)
                if hit is None:
                    row.update({
                        "command": None,
                        "exit_code": None,
                        "signal": f"{case['test']}: no result recorded",
                        "executable": built["executable"],
                        "executable_sha256": built["executable_sha256"],
                        "log": None,
                        "ran": False,
                    })
                else:
                    row.update({
                        "command": hit["command"],
                        "exit_code": hit["exit_code"],
                        "signal": hit["signal"],
                        "executable": built["executable"],
                        "executable_sha256": built["executable_sha256"],
                        "log": hit["log"],
                        "ran": True,
                    })
            observations.append(row)
observations.sort(key=lambda o: (
    order_rev.index(o["revision_id"]),
    order_profile.index(o["profile"]),
    case_index[(o["revision_id"], o["case"])],
))
refused = [r for r in runs if not r["source_ok"]]
expected = len(order_profile) * sum(len(rev["cases"]) for rev in manifest["revisions"])
if fail != "0" or refused:
    print(f"runner_failed={fail != '0'} refused={len(refused)}", file=sys.stderr)
    for row in refused:
        print(f"{row['revision_id']}: {row['source_note']}", file=sys.stderr)
    raise SystemExit(1)
if len(observations) != expected:
    raise SystemExit(f"expected {expected} observations, got {len(observations)}")
manifest["observations"] = observations
rustc = subprocess.check_output(["rustc", "--version"], text=True).strip()
manifest["replay"] = {
    "rustc": rustc,
    "case_timeout_s": int(__import__("os").environ.get("CASE_TIMEOUT_S", "600")),
    "finished_unix": int(datetime.datetime.now(datetime.timezone.utc).timestamp()),
    "finished_utc": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
    "runner_failed": fail != "0",
    "runs": runs,
}
tmp = pathlib.Path(manifest_path + ".tmp")
tmp.write_text(json.dumps(manifest, indent=2) + "\n")
tmp.replace(manifest_path)
print(f"observations {len(observations)}")
for o in observations:
    code = o["exit_code"]
    print(f"{o['revision_id']} {o['profile']} {o['case']} exit={code} {o['signal']}")
PY

rm -f "$LOCK"
exit "$fail"

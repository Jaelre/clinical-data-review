#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${repo_root}"

fail() {
    printf 'public-tree check failed: %s\n' "$1" >&2
    exit 1
}

planning_artifact="con""ductor"
encrypted_history_tool="git""-crypt"

if git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
    mapfile_command=(git ls-files -z)
else
    mapfile_command=(find . -type f -not -path './target/*' -not -path './apps/review/node_modules/*' -print0)
fi

tracked_files=()
while IFS= read -r -d '' path; do
    tracked_files+=("${path#./}")
done < <("${mapfile_command[@]}")

for path in "${tracked_files[@]}"; do
    case "${path}" in
        .env.example|fixtures/synthetic/*.xlsx)
            ;;
        .env|*.env|*.db|*.db-*|*.sqlite|*.sqlite-*|*.sqlite3|*.sqlite3-*|*.xls|*.xlsx|*.xlsm|*.doc|*.docx|*.pdf|*.parquet|*.zip|*.tar|*.tgz|*.gz|*.7z|*.bak|*.backup|*.png|*.jpg|*.jpeg|*.gif|*.webp|*.bmp|*.tif|*.tiff|*.ico|*.icns|*.woff|*.woff2|*.ttf|*.otf|*.wasm|*.bin|*.exe|*.dll|*.dylib|*.so|*.a)
            fail "forbidden tracked data or archive file: ${path}"
            ;;
    esac
    case "${path}" in
        AGENTS.md|.gitmodules|*"${planning_artifact}"*|*"${encrypted_history_tool}"*|bundles/*|*/bundles/*|backups/*|*/backups/*)
            fail "private or development artifact is tracked: ${path}"
            ;;
    esac
done

if find . -mindepth 2 -type d -name .git -not -path './target/*' -print -quit | grep -q .; then
    fail "nested Git repository found"
fi

content_files=()
for path in "${tracked_files[@]}"; do
    case "${path}" in
        Cargo.lock|scripts/check-public-tree.sh|*.png|*.ico|*.icns|fixtures/synthetic/*.xlsx)
            ;;
        *)
            content_files+=("${path}")
            ;;
    esac
done

if ((${#content_files[@]} > 0)); then
    private_pattern="supa""base|${encrypted_history_tool}|codi""ciminori|csv_""Edit_scripts|NUMERO_""PRATICA_PS|ID_""progressivo|PS_[A-Z_]+\\.|pra""tica|pro""gressivo|co""gnome|se""sso|ge""nere|workspace\\.local|local-""reviewer"
    if rg -n -i \
        "${private_pattern}" \
        "${content_files[@]}"; then
        fail "private platform, legacy project, or source-system marker found"
    fi
fi

if cargo tree -i sqlx-postgres -e normal,build,dev 2>/dev/null | rg -q 'sqlx-postgres'; then
    fail "a PostgreSQL backend is present in the resolved dependency graph"
fi

python3 scripts/check-synthetic-workbooks.py fixtures/synthetic
printf 'public-tree checks passed (%s files inspected)\n' "${#tracked_files[@]}"

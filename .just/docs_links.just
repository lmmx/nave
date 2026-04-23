check-docs-links:
    #!/usr/bin/env bash
    set -euo pipefail

    echo "Checking markdown internal links..."

    fail_file="$(mktemp)"
    trap 'rm -f "$fail_file"' EXIT

    while read -r file; do

        dir=$(dirname "$file")

        while read -r link; do

            [[ "$link" =~ ^https?:// ]] && continue

            path="$link"
            anchor=""

            if [[ "$link" == *"#"* ]]; then
                path="${link%%#*}"
                anchor="${link#*#}"
            fi

            # -----------------------------
            # HANDLE SAME-FILE ANCHORS
            # -----------------------------
            if [[ -z "$path" ]]; then
                resolved_file="$file"
                docs_path="${file#*/docs/}"
            else
                resolved="$(realpath -m "$dir/$path")"

                if [[ "$resolved" != *"/docs/"* ]]; then
                    echo "❌ invalid link (outside docs): $file → $link"
                    echo 1 > "$fail_file"
                    continue
                fi

                resolved_file="$resolved"
                docs_path="${resolved#*/docs/}"
            fi

            # -----------------------------
            # FILE EXISTENCE CHECK
            # -----------------------------
            if [[ ! -f "$resolved_file" ]]; then
                echo "❌ broken link:"
                echo "   file: $file"
                echo "   link: $link"
                echo "   resolved: $resolved_file"
                echo 1 > "$fail_file"
                continue
            fi

            # -----------------------------
            # ANCHOR VALIDATION
            # -----------------------------
            if [[ -n "$anchor" ]]; then
                slugs=$(grep -E '^#+ ' "$resolved_file" \
                    | sed -E 's/^#+ //' \
                    | tr '[:upper:]' '[:lower:]' \
                    | sed -E 's/[^a-z0-9]+/-/g; s/^-+|-+$//g')

                if ! echo "$slugs" | grep -qx "$anchor"; then
                    echo "❌ broken anchor:"
                    echo "   file: $file"
                    echo "   link: $link"
                    echo "   anchor not found in: $resolved_file"
                    echo 1 > "$fail_file"
                fi
            fi

        done < <(
            grep -oE '\[.*?\]\(([^)]+)\)' "$file" \
            | sed -E 's/.*\(([^)]+)\).*/\1/' || true
        )

    done < <(find docs -name "*.md")

    if [[ -s "$fail_file" ]]; then
        echo "Link check failed"
        exit 1
    fi

    echo "All links valid"

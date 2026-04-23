check-docs-nav:
    #!/usr/bin/env -S echo-comment --shell-flags="-e" --color bright-green
    echo "Checking zensical nav vs docs/..."

    nav_files=$(tq -f zensical.toml 'project.nav' -o json | jq -r '.. | strings' | sort -u)

    md_files=$(find docs -name "*.md" | sed 's|docs/||' | sort -u)

    missing=$(comm -23 <(echo "$md_files") <(echo "$nav_files"))

    if [ -n "$missing" ]; then
        echo "missing in nav:"
        echo "$missing"
        exit 1
    fi

    # Nav ok

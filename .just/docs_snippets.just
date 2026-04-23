# Generate CLI help snippets
snippets:
    @echo "Generating CLI snippets..."

    mkdir -p docs/_snippets/cli/pen

    # top-level commands
    for cmd in init scan pull check build schemas search pen; do \
        echo "→ $cmd"; \
        nave $cmd --help | sed 's/\x1b\[[0-9;]*m//g' > docs/_snippets/cli/$cmd.txt; \
    done

    # pen subcommands
    for cmd in create list show status sync clean revert reinit exec rm; do \
        echo "→ pen $cmd"; \
        nave pen $cmd --help | sed 's/\x1b\[[0-9;]*m//g' > docs/_snippets/cli/pen/$cmd.txt; \
    done

# Check every snippet has matching markdown doc
check-docs-snippets:
    #!/usr/bin/env -S echo-comment --shell-flags="-e" --color bright-green
    echo "Checking snippets → docs..."

    missing=0

    for f in docs/_snippets/cli/*.txt; do
        name=$(basename $f .txt)

        if [[ $f == *"/pen/"* ]]; then
            md="docs/reference/cli/pen/$name.md"
        elif [[ $name == "pen" ]]; then
            md="docs/reference/cli/pen.md"
        else
            md="docs/reference/cli/$name.md"
        fi

        if [ ! -f $md ]; then
            # MISSING: $md
            missing=1
        fi
    done

    if [ $missing -ne 0 ]; then
        exit 1
    fi

    # OK: all docs present

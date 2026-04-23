# nave build

Simplify configuration drift across repositories by extracting shared structure.

## Usage

--8<-- "_snippets/cli/build.txt"

## Concept

Build performs structural comparison across configs of the same type.

It identifies:

- shared templates
- optional fields
- divergent values

This is used to understand configuration drift at scale.

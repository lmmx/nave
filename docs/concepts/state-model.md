# State model

Nave is built around explicit system states.

## Core states

- Fleet: all repos
- Scan: raw GitHub index (no files)
- Cache: local tracked files
- Analysis: derived outputs
- Pen: mutable workspace

## Flow

scan → cache → analysis → pen

## Rules

scan: no files
pull: no logic
search: no writes
pen: only writer

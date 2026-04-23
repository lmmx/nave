# Lifecycle

Nave follows a simple operational pipeline:

## 1. init
Configure authentication and tracked paths.

## 2. scan
Discover repositories in your fleet and index relevant files.

## 3. pull
Materialise a local cache via sparse checkout.

## 4. analyse
Search, validate, and transform structured data across repos.

## 5. operate
Use pens to perform scoped changes across multiple repositories.

This lifecycle is designed to separate discovery, analysis, and mutation cleanly.

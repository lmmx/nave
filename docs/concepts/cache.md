# Cache model

Nave builds a local cache after scan + pull.

Flow:

- scan → repo list
- pull → sparse checkout
- cache → local files

Cache = analysis source.

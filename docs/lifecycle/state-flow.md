# State flow

scan → repo index
pull → local cache
search/build → analysis
pen → mutation layer

## invariants
scan: no write
pull: no analysis
search: no write
pen: only write

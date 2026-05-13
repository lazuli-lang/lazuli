This recipe records the additive `SourceMap` sidecar companion introduced in
LZIR schema `0.12.0`.

There is no author-source rewrite and no `Module` JSON shape change. Consumers
that read the IR JSON ABI should become aware that source-aware runs may emit a
paired `<module>.sourcemap.json` sidecar, but projects do not need to re-author
any `.lzi` or `.lzx` files for this migration.

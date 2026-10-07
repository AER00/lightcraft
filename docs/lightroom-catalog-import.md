# Lightroom Classic catalog import

File → Import Lightroom Catalog… opens `.lrcat` directly. Lightroom, Python, DNG conversion
and a C SQLite runtime are not required. The source database is read-only, including committed
`-wal` pages; a catalog that changes during the read is rejected. LightCraft owns subsequent edits.

Equivalent commands:

```text
lightcraft-cli run --library "LightCraft Library" library.inspectLightroom path="Lightroom Catalog.lrcat"
lightcraft-cli run --library "LightCraft Library" library.importLightroom path="Lightroom Catalog.lrcat"
```

Original files are referenced in place. Ratings, picks/rejects, color labels, XMP metadata,
hierarchical keywords, regular collections/collection sets, virtual copies and mapped develop
settings are imported. Missing originals remain catalogued for relinking. Existing LightCraft
edits are preserved by default; `updateExisting=true` explicitly replaces them. Persistent import
identity prevents reimport from duplicating virtual copies and collections. One undo step reverses
catalog changes; recovery archives remain available.

Before changing records, source image records, decoded XMP, verbatim develop settings, history,
snapshots and collection content are saved to `Interop/lightroom-import-N.json` in the LightCraft
library. `Interop/lightroom-index.json` records imported identities. Source catalogs and originals
are never overwritten. Native import is bounded to a 1 GiB database/WAL, 16 MiB XMP packets and
one million rows per table. The SQLite reader is Apache-2.0 `sqlite-core`, written in Rust.

Develop settings reuse the existing XMP/preset mapper. Supported sliders, curves and supported
mask structures remain editable, but this is approximate rendering: camera profiles, Adobe AI
models, some masking/retouch fields and process-version algorithms are not reproduced. Unmapped
fields are reported; source settings/history/snapshots are archived rather than discarded. Smart
collections become regular albums with current membership; their original rules remain archived.
History and snapshots are retained as source data, not exposed as native LightCraft history yet.
Lightroom's `-999999` deferred-adjustment sentinel is omitted from both catalog and XMP mappings;
it is reported and archived, never clamped into a real slider value. Deferred Adobe Auto Tone
is not evaluated by the importer; LightCraft's Auto control remains available after migration.

The importer uses shared Rust code on macOS, Windows and Linux. If originals move between
computers or volumes, use LightCraft's missing-photo relinking tools to update their paths.

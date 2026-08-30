# PADAGONIA Integration Roadmap

Use `/home/sal/padagonia/docs/enterprise-integration-directives.md`.

## Modules

- `snapshot_event_adapter`: record bundle IDs, files, languages, dependency
  edges, token estimates, and consumer project.
- `context_provenance_writer`: preserve why each file entered a bundle and the
  source revision/hash.
- `reuse_reader`: find prior bundles and semantically similar context.
- `retention`: expire raw snapshots while retaining approved usage metadata.

## Acceptance gates

Identical inputs produce idempotent graph state, bundle provenance is complete,
and Padagonia outages do not block local bundling.

# Simplified Autoflow Plan

## User-facing UI

Only keep:

1. Upload video
2. Download final edited video

## Hidden background pipeline

The existing tested flow should run automatically behind the new frontend:

1. Create project
2. Import original video
3. Generate proxy and audio assets
4. Run analyzer automatically
5. Apply default review/export path automatically
6. Produce final downloadable output

## Recommended user states

Because the UI becomes very simple, the frontend still needs a small set of states:

- Idle
- Uploading
- Processing
- Exporting
- Ready to download
- Failed

## Integration note

The old product exposed multiple internal stages to the UI. V2 should invert that relationship:

- frontend triggers one high-level job
- backend orchestrates the old multi-step pipeline
- frontend only polls/subscribes to overall status and final file readiness

## What I need from the handoff frontend

- source files or export bundle
- button mapping for upload/download
- loading/progress behavior
- responsive behavior for desktop/mobile

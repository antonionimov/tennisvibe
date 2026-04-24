# Tennis Auto Editor V2

A brand new standalone project for the next generation of the tennis auto editing tool.

## Goal

Build a redesigned product that can eventually ship as installable apps for:

- Windows
- macOS
- Android
- iPhone

## Why this is a new project

Per workspace rule, this work lives in its own folder instead of overwriting the current MVP.

- Existing app stays untouched in `apps/tennis-auto-editor`
- New redesign and packaging work happens in `apps/tennis-auto-editor-v2`

## Proposed stack

- Frontend: React + Vite
- App shell: Tauri v2
- Desktop targets: Windows, macOS
- Mobile targets: Android, iOS

## Important architecture note

A direct mobile release is not just a UI packaging problem.

The current product pipeline depends on desktop-style local tooling, especially Python analysis and ffmpeg-heavy local processing. That is manageable on desktop, but much riskier on iPhone and Android.

So V2 should be split into two layers:

1. **Cross-platform product shell**
   - project list
   - import flow
   - analysis/review UI
   - export UI
   - settings
2. **Portable processing core strategy**
   - either migrate critical pipeline logic away from Python-only assumptions
   - or clearly separate desktop-local processing from future mobile-compatible execution

## First phase

Phase 1 is to design and implement a brand new frontend shell and information architecture.

Current V2 design direction:

- cleaner studio layout
- clear project states
- bigger review workspace
- cross-platform responsive structure from day one
- mobile-friendly navigation model

## Next steps

1. Finalize V2 frontend information architecture
2. Build the first redesigned UI shell
3. Map desktop-only pipeline pieces
4. Decide mobile strategy for analysis/export engine
5. Enable packaging per platform

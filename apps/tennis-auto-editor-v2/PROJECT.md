# Project Plan

## Project name

Tennis Auto Editor V2

## Mission

Create a new generation of the tennis auto editing product with:

1. a brand new frontend
2. a packaging path for Windows, macOS, Android, and iPhone

## Non-goal for the first step

Do not mutate the current MVP in `apps/tennis-auto-editor`.

## Product strategy

### Step A: redesign the product shell first

We redesign the UX before touching final packaging.

Core screens for V2:

- Dashboard
- Project creation/import
- Analysis overview
- Review timeline
- Export center
- Settings / platform status

### Step B: separate UI from processing engine

To truly support desktop and mobile, we should stop treating the current desktop pipeline as the app boundary.

Recommended split:

- **UI shell**: cross-platform React/Tauri app
- **processing engine**: portable core plus platform-specific execution strategy

## Big technical warning

If the app must run locally on iPhone and Android, the current desktop-style Python + ffmpeg assumptions will likely need refactoring.

That means the frontend redesign can start immediately, but the packaging track should be treated as two layers:

- packaging the app shell
- making the media pipeline mobile-compatible

## Immediate deliverables

- new standalone project folder
- first-pass frontend architecture
- first-pass visual direction
- packaging roadmap

## Suggested milestones

### M1
- create new standalone project
- write product brief
- define screens and design direction

### M2
- build redesigned frontend shell with mock data
- make it responsive for desktop and mobile

### M3
- connect frontend shell to project and review data model

### M4
- split desktop-only processing from portable interfaces

### M5
- enable desktop packaging first: Windows + macOS

### M6
- evaluate and implement Android + iOS strategy

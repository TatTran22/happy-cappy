# Happy Cappy Smoke Results

Date: 2026-05-25
Bundle: `dist/Happy Cappy.app`

## Automated Checks

- `./scripts/verify.sh`: PASS
- `./scripts/smoke_app.sh` static checks and fresh process launch check: PASS

## Manual Checks

- No Dock icon and no Cmd+Tab entry: NOT VERIFIED
- Menu bar item title is `HC`: NOT VERIFIED
- Settings opens from menu bar: NOT VERIFIED
- Personality changes apply on hover: NOT VERIFIED
- Drag persists after quit and relaunch: NOT VERIFIED
- Right-click visible pet pixels opens context menu: NOT VERIFIED
- Interactive mode supports hover, drag, and right-click: NOT VERIFIED
- Focus Mode passes clicks through to apps underneath: NOT VERIFIED
- Menu bar disables Focus Mode and restores interactions: NOT VERIFIED
- Nap shows sleepy action and pauses walking temporarily: NOT VERIFIED
- Cheer Up shows happy action temporarily: NOT VERIFIED
- Hide Pet keeps menu bar app alive: NOT VERIFIED
- Show Pet restores the pet: NOT VERIFIED
- Reset Position returns the pet to a visible safe location: NOT VERIFIED

## Notes

- `./scripts/smoke_app.sh` closed any pre-existing `happy-cappy` process, launched the app successfully after static bundle checks, and confirmed a fresh `happy-cappy` process stayed running.
- Interactive mode intentionally uses full-frame window event capture for reliable controls.
- Focus Mode is the supported click-through mode.
- Manual UI checks require direct user observation and were not marked pass automatically.

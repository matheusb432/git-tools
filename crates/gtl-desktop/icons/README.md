# Repository Hub icon

The three connected repository blocks are defined in the compact SVG at `crates/gtl-web/src/app/assets/repository-hub.svg`.

Run `just desktop gen-icon` to regenerate every icon from that mark and the theme tokens in `crates/gtl-web/src/app/assets/styles/tokens.css`.
It writes the default launcher SVG, 1024px PNG, 512px macOS bundle PNG, seven-resolution ICO, and web favicon SVG in the Dark theme's surface, outline, and accent.
The macOS bundle uses the 512px image because ICNS does not support a standard-density 1024px image.
It also writes `themes/<theme>/` with a launcher SVG, a 512px launcher PNG, and a 64px tray PNG of the bare mark in each theme's accent.

The bundle, favicon, and first tray image use the Dark theme.
Once the viewer loads its theme, the desktop shell swaps in that theme's tray icon and, on Linux, rewrites the launcher SVG and PNG that `just install` placed in the hicolor icon theme, so GNOME's dock and app grid follow it.
The viewer's navigation logo draws the same geometry inline from the compact mark with the active theme's surface, line, and accent tokens.
Intrinsic SVG dimensions keep the navigation mark bounded before CSS loads.

Theme proposals, mockups, and screenshots belong under the ignored `.artifacts/` directory; they are not production assets and must not be committed.

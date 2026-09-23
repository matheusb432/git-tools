# Repository Hub icon

The three connected repository blocks are defined in the compact SVG at `crates/gtl-web/src/app/assets/repository-hub.svg`.

Run `just desktop gen-icon` to regenerate the purple-on-dark launcher SVG, 1024px PNG, 512px macOS bundle PNG, seven-resolution ICO, web favicon SVG, and monochrome tray SVG/PNG. The macOS bundle uses the 512px image because ICNS does not support a standard-density 1024px image. Linux installation includes the scalable SVG and a 512px PNG fallback. Intrinsic SVG dimensions keep the navigation mark bounded before CSS loads.

The tray keeps its light silhouette. The launcher, favicon, and dashboard logo use the default Dark theme's purple accent and dark surface, independently of the selected application theme. Theme proposals, mockups, and screenshots belong under the ignored `.artifacts/` directory; they are not production assets and must not be committed.

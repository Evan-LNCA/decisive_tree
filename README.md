# Decisive Tree

A fast, single-file flowchart and decision-tree editor for modeling manufacturing workflows.
The default styles match the Redtag Process chart: yellow process boxes, blue/teal/lavender pills,
orange decision diamonds, red/green outcomes, and grey right-angle connectors.

## Sharing

`decisive_tree.exe` is fully standalone (no installer, no runtime). Send that one file to coworkers.
Diagrams save as `.dtree` files (JSON). Open one by dragging it onto the window or onto the exe.

## Quick use

| Action | How |
|---|---|
| Add a step | Double-click empty canvas, or right-click → Add shape here |
| Add the next step / a branch | Select a node, then **Tab** (right) or **Shift+Tab** (below) |
| Connect two nodes | Hover a node, drag one of its blue port dots onto another node (Alt+drag a node works too) |
| Connect to a new step | Drag a port dot onto empty space |
| Re-route a connector | Click it, then drag its end dot to another node, blue port dot, or side midpoint |
| Move a connector's bend | Click it, then drag the white square — with Snap enabled, it aligns with parallel connector segments and node midpoints; hold Alt to place freely |
| Move a connector's label | Drag the label along the line — it snaps to segment midpoints (hold Alt to place freely; "Auto label position" resets) |
| Edit text | Double-click, or Enter / F2. Enter finishes, Shift+Enter adds a new line |
| Style | Select nodes, then use the right panel (shape, fill, border, font, size, bold, underline, shadow). Clicking a palette preset applies it |
| Select | Click, Shift+click, or drag a box on empty space |
| Pan / zoom | Mouse wheel / middle-drag / Space+drag. Ctrl+wheel zooms. Ctrl+0 fits the diagram |
| Copy / paste / duplicate | Ctrl+C / Ctrl+V / Ctrl+D. Copy style with Ctrl+Shift+C / V |
| Undo / redo | Ctrl+Z / Ctrl+Y |
| Export | PNG / PDF / SVG buttons in the toolbar (Ctrl+E = PNG) |

Several connectors leaving one node (decisions) or entering one node (merges) automatically share a
clean trunk. Pin a connector's exit/entry side in the right panel if you want a specific layout.
Connector ends snap to side midpoints on every node shape, including boxes and circles.
Pink guides show bend alignment; alignment takes priority over the grid.

## Building

Requires Rust (stable) and the MSVC build tools.

```
cargo build --release
```

Output: `target\release\decisive_tree.exe`. The app uses OpenGL by default. If OpenGL is not available
(for example on some remote desktops or VMs), it automatically restarts with DirectX/Vulkan.
To force that mode, run `decisive_tree.exe --renderer=wgpu`.

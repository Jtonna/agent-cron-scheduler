/**
 * cursors
 *
 * High-contrast SVG cursors for the workflow graph editor.
 *
 * Why this exists: the editor canvas uses a near-white background
 * (bg-surface-tertiary with a dot grid). Users running OSX-style mostly
 * white cursor themes (including on Windows) end up with an invisible
 * white "hand" cursor over nodes and edges. To guarantee visibility
 * regardless of the user's OS cursor theme, we ship our own SVG
 * cursors with a black stroke and white fill so they're crisp on both
 * light and dark surfaces.
 *
 * Color note: the SVG fill is `#fff` and the stroke is `#000`. These
 * are intentionally HARDCODED (not tokens) — cursor visuals must look
 * the same regardless of light/dark theme, and the cursor needs to
 * stand out against any canvas background. This is the one place in
 * the editor where a literal color override is the right call.
 *
 * The CSS values include a system fallback (`, pointer` / `, grab`)
 * so the cursor remains correct even if the data-URI fails to render.
 */

// Standard arrow pointer: black outline, white fill, hotspot at the tip.
// Kept small (16x16) so the OS renders it at native crispness.
const POINTER_SVG =
  '<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 16 16">' +
  '<path d="M1.5 1.2 L1.5 13.4 L4.6 10.6 L6.5 14.8 L8.5 13.9 L6.6 9.8 L11 9.6 Z" ' +
  'fill="#fff" stroke="#000" stroke-width="1.2" stroke-linejoin="round"/>' +
  "</svg>";

// Grab hand silhouette: a stubby four-finger open hand with the same
// black stroke + white fill treatment. 20x20 for a bit more legibility.
const GRAB_SVG =
  '<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 20 20">' +
  '<path d="M5 11 L5 7.5 a1 1 0 0 1 2 0 L7 10 L7 5 a1 1 0 0 1 2 0 L9 10 L9 4.5 ' +
  "a1 1 0 0 1 2 0 L11 10 L11 5.5 a1 1 0 0 1 2 0 L13 11.5 L14.5 10 " +
  "a1 1 0 0 1 1.4 1.4 L13 16 a3 3 0 0 1 -2.6 1.5 L8 17.5 " +
  'a3 3 0 0 1 -3 -3 Z" fill="#fff" stroke="#000" stroke-width="1.1" stroke-linejoin="round"/>' +
  "</svg>";

// Grabbing hand (closed fist outline). Same palette.
const GRABBING_SVG =
  '<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 20 20">' +
  '<path d="M5 12 L5 9 a1 1 0 0 1 2 0 L7 8.5 a1 1 0 0 1 2 0 L9 8.5 ' +
  "a1 1 0 0 1 2 0 L11 9 a1 1 0 0 1 2 0 L13 12 a3 3 0 0 1 -3 3 L8 15 " +
  'a3 3 0 0 1 -3 -3 Z" fill="#fff" stroke="#000" stroke-width="1.1" stroke-linejoin="round"/>' +
  "</svg>";

const enc = (svg: string): string =>
  encodeURIComponent(svg).replace(/'/g, "%27").replace(/"/g, "%22");

export const CURSOR_POINTER_URL = `url("data:image/svg+xml;utf8,${enc(POINTER_SVG)}") 1 1, pointer`;
export const CURSOR_GRAB_URL = `url("data:image/svg+xml;utf8,${enc(GRAB_SVG)}") 10 10, grab`;
export const CURSOR_GRABBING_URL = `url("data:image/svg+xml;utf8,${enc(GRABBING_SVG)}") 10 10, grabbing`;

/**
 * CSS block scoped to `[data-acs-editor]` that forces a high-contrast
 * cursor on the reactflow canvas and every interactive element within
 * the editor surface (canvas, nodes, edges, minimap, controls). The
 * `!important` flags are required because reactflow ships its own
 * cursor rules on `.react-flow__pane` and `.react-flow__node`.
 *
 * Our own Tailwind `cursor-pointer` / `cursor-grab` classes already
 * cover hover-actions, palette chips, modal buttons, etc.; the rules
 * below piggy-back on those by applying the SVG cursor to any
 * descendant that has explicitly opted into a pointer/grab cursor.
 */
export const EDITOR_CURSOR_CSS = `
  [data-acs-editor] .react-flow__pane {
    cursor: ${CURSOR_GRAB_URL} !important;
  }
  [data-acs-editor] .react-flow__pane.dragging,
  [data-acs-editor] .react-flow__pane:active {
    cursor: ${CURSOR_GRABBING_URL} !important;
  }
  [data-acs-editor] .react-flow__node {
    cursor: ${CURSOR_POINTER_URL} !important;
  }
  [data-acs-editor] .react-flow__node.dragging,
  [data-acs-editor] .react-flow__node:active {
    cursor: ${CURSOR_GRABBING_URL} !important;
  }
  [data-acs-editor] .react-flow__edge,
  [data-acs-editor] .react-flow__edge-path,
  [data-acs-editor] .react-flow__edge-interaction {
    cursor: ${CURSOR_POINTER_URL} !important;
  }
  [data-acs-editor] .react-flow__minimap {
    cursor: ${CURSOR_POINTER_URL} !important;
  }
  [data-acs-editor] .react-flow__controls-button {
    cursor: ${CURSOR_POINTER_URL} !important;
  }
  /* Piggy-back on Tailwind's cursor utilities used throughout the
     editor (hover actions, palette chips, modal buttons, breadcrumb,
     edge + button, etc.) so they get the same crisp SVG cursor. */
  [data-acs-editor] .cursor-pointer {
    cursor: ${CURSOR_POINTER_URL} !important;
  }
  [data-acs-editor] .cursor-grab {
    cursor: ${CURSOR_GRAB_URL} !important;
  }
  [data-acs-editor] .cursor-grabbing,
  [data-acs-editor] .cursor-grab:active {
    cursor: ${CURSOR_GRABBING_URL} !important;
  }
`;

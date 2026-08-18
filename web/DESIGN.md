---
name: Flux Purr Web Console
description: A restrained industrial instrument panel for operating one Flux Purr device.
colors:
  chassis: "oklch(0.928 0.014 248)"
  panel: "oklch(0.968 0.007 235)"
  recessed: "oklch(0.89 0.018 241)"
  ink: "oklch(0.302 0.026 255)"
  label-ink: "oklch(0.392 0.022 252)"
  signal-red: "oklch(0.58 0.209 25)"
  signal-red-glow: "oklch(0.676 0.18 22)"
  shadow: "oklch(0.79 0.02 248)"
  deep-shadow: "oklch(0.738 0.023 247)"
  highlight: "oklch(0.996 0.003 220)"
  technical-charcoal: "oklch(0.302 0.026 255)"
  technical-slate: "oklch(0.344 0.033 251)"
  status-green: "oklch(0.72 0.154 151)"
  warning-yellow: "oklch(0.83 0.148 92)"
typography:
  display:
    fontFamily: "Manrope, Noto Sans SC, sans-serif"
    fontSize: "clamp(1.85rem, 3vw, 2.55rem)"
    fontWeight: 800
    lineHeight: 1
    letterSpacing: "0"
  title:
    fontFamily: "Manrope, Noto Sans SC, sans-serif"
    fontSize: "1.35rem"
    fontWeight: 800
    lineHeight: 1.1
    letterSpacing: "0"
  body:
    fontFamily: "Manrope, Noto Sans SC, sans-serif"
    fontSize: "1rem"
    fontWeight: 600
    lineHeight: 1.45
    letterSpacing: "0"
  label:
    fontFamily: "JetBrains Mono, monospace"
    fontSize: "0.7rem"
    fontWeight: 800
    lineHeight: 1.2
    letterSpacing: "0.06em"
rounded:
  compact: "8px"
  control: "14px"
  module: "18px"
  panel: "22px"
  housing: "28px"
  pill: "9999px"
spacing:
  xs: "0.42rem"
  sm: "0.7rem"
  md: "0.85rem"
  lg: "1rem"
  xl: "1.25rem"
components:
  button-primary:
    backgroundColor: "{colors.signal-red}"
    textColor: "#ffffff"
    typography: "{typography.label}"
    rounded: "{rounded.control}"
    padding: "0.75rem 1rem"
    height: "48px"
  button-secondary:
    backgroundColor: "{colors.chassis}"
    textColor: "{colors.ink}"
    typography: "{typography.label}"
    rounded: "{rounded.control}"
    padding: "0.75rem 1rem"
    height: "48px"
  navigation-key:
    backgroundColor: "{colors.chassis}"
    textColor: "{colors.label-ink}"
    rounded: "{rounded.control}"
    padding: "0.78rem 0.9rem"
    height: "48px"
  data-input:
    backgroundColor: "{colors.chassis}"
    textColor: "{colors.ink}"
    typography: "{typography.label}"
    rounded: "{rounded.module}"
    padding: "0.62rem 0.72rem"
    height: "44px"
  raised-panel:
    backgroundColor: "{colors.chassis}"
    textColor: "{colors.ink}"
    rounded: "{rounded.panel}"
    padding: "1rem"
  status-pill:
    backgroundColor: "{colors.panel}"
    textColor: "{colors.label-ink}"
    typography: "{typography.label}"
    rounded: "{rounded.pill}"
    padding: "0.45rem 0.65rem"
---

# Design System: Flux Purr Web Console

## Overview

**Creative North Star: "The Instrument Panel"**

Flux Purr Web Console behaves like a compact bench instrument: every surface has physical position, every state has operational meaning, and the operator can scan a dense workspace without mistaking decoration for capability. Its industrial skeuomorphism comes from material logic rather than nostalgia.

The system is tactile but disciplined. Raised controls invite manipulation, recessed fields and displays receive information, and dark technical surfaces isolate logs or device readouts. Screws, scanlines, texture, and LED glow are signature details only when they reinforce mounting, material, or status.

**Key Characteristics:**

- Light-mode cool-grey chassis with restrained signal color.
- Top-left illumination and structurally paired highlights and shadows.
- Recognizable controls with explicit hover, focus, pressed, selected, and disabled states.
- Dense single-device workbench layouts that preserve transport and safety context.
- Technical data in mono; interface language in a compact humanist sans.

## Colors

The palette describes workshop materials and operational signals rather than brand decoration.

### Primary

- **Signal Red** (oklch(0.58 0.209 25)): The sole interaction accent for current selection, primary action, focus, and danger. Its scarcity preserves urgency.
- **Signal Red Glow** (oklch(0.676 0.18 22)): A brighter emitted edge for active red LEDs and critical feedback, never a surface fill.

### Secondary

- **Operational Green** (oklch(0.72 0.154 151)): Nominal or successful device state only.
- **Warning Yellow** (oklch(0.83 0.148 92)): Caution and degraded state only.

### Neutral

- **Cool Chassis** (oklch(0.928 0.014 248)): The continuous material base.
- **Raised Panel** (oklch(0.968 0.007 235)): A lighter mounted surface.
- **Recessed Grey** (oklch(0.89 0.018 241)): Inputs, grooves, and sunken controls.
- **Instrument Ink** (oklch(0.302 0.026 255)): Primary text and silhouettes.
- **Stamped Label Ink** (oklch(0.392 0.022 252)): Metadata and secondary text.
- **Technical Charcoal** (oklch(0.302 0.026 255)): The dark base for logs and screens.
- **Technical Slate** (oklch(0.344 0.033 251)): The lighter end of dark technical modules.
- **Structural Shadow** (oklch(0.79 0.02 248)): The dark half of the lighting model.
- **Deep Structural Shadow** (oklch(0.738 0.023 247)): Strong separation for large housings and dark modules.
- **Top-left Highlight** (oklch(0.996 0.003 220)): The light half of the lighting model.

**The Signal Discipline Rule.** Signal red owns interaction emphasis; green and yellow communicate device state and never become ordinary decorative accents.

**The Live-State Rule.** Live, mock, offline, degraded, and unsupported states must differ through words or shape as well as color.

## Typography

**Display Font:** Manrope (with Noto Sans SC and sans-serif fallback)

**Body Font:** Manrope (with Noto Sans SC and sans-serif fallback)

**Label/Mono Font:** JetBrains Mono (with monospace fallback)

**Character:** Manrope keeps the Chinese-first operating surface compact and readable without becoming generic. JetBrains Mono gives numbers, transport identifiers, timestamps, badges, and technical controls the precision of stamped instrument labeling.

### Hierarchy

- **Display** (Manrope, weight 800, responsive 1.85rem to 2.55rem, line-height 1): Heavy, compact product and workspace identity; never marketing-scale inside the console.
- **Title** (Manrope, weight 800, 1.35rem, line-height 1.1): Strong module headings with a subtle embossed highlight.
- **Body** (Manrope, weight 600, 1rem, line-height 1.45): Dense but readable operating copy, normally constrained to short instructions and status explanations.
- **Label** (JetBrains Mono, weight 800, 0.7rem, line-height 1.2): Uppercase where appropriate, wide-set, and reserved for metadata, measurements, badges, and control legends.

**The Data Face Rule.** Measurements, identifiers, timestamps, and machine state use the mono face; prose and navigation stay in the humanist face.

## Layout

The application fills the dynamic viewport and centers a bounded instrument chassis. Inside it, identity and view controls lead into a workspace that pairs the active task with a persistent event surface on wide screens. At narrower widths, grids collapse, control groups stack, and the event surface compresses without abandoning the physical metaphor.

Spacing is compact and repetitive: the small and medium rhythm separates controls, while larger gaps separate functional modules. Repeated controls use stable tracks, minimum heights, and bounded panels so state changes do not shift the surrounding layout. Interactive targets remain at least 48px on mobile.

**The One Bench Rule.** Keep one active device and its immediate workflow in view; do not turn the console into a fleet dashboard or a field of unrelated cards.

## Elevation & Depth

Depth is structural. A fixed top-left light source creates light top/left edges and dark bottom/right shadows. Raised modules use paired outer shadows; inputs, selected controls, and pressed keys invert those pairs into inset shadows. Dark technical panels may add a restrained inner rim or scanline layer, while LED glow is reserved for emitted status light.

### Shadow Vocabulary

- **Chassis Lift** (`box-shadow: 10px 14px 28px color-mix(in oklch, oklch(0.738 0.023 247) 28%, transparent), -4px -4px 10px color-mix(in oklch, oklch(0.996 0.003 220) 62%, transparent)`): The console separates from the page with a broad, low-contrast paired shadow.
- **Module Lift** (`box-shadow: 8px 8px 16px oklch(0.79 0.02 248), -8px -8px 16px oklch(0.996 0.003 220)`): Panels and cards read as mounted plastic.
- **Control Lift** (`box-shadow: 5px 5px 10px oklch(0.79 0.02 248), -5px -5px 10px oklch(0.996 0.003 220)`): Buttons and navigation keys rise slightly on hover.
- **Pressed** (`box-shadow: inset 6px 6px 12px oklch(0.79 0.02 248), inset -6px -6px 12px oklch(0.996 0.003 220)`): Active controls reverse the shadow pair inward.
- **Recessed** (`box-shadow: inset 4px 4px 8px oklch(0.79 0.02 248), inset -4px -4px 8px oklch(0.996 0.003 220)`): Inputs and data wells sit below the chassis.
- **Signal Glow** (`box-shadow: 0 0 10px 2px rgba(255, 71, 87, 0.55)`): Colored bloom belongs only to status emitters and focus-critical feedback.

**The Fixed-Light Rule.** Highlights always come from the top-left; mixed lighting breaks the instrument illusion.

**The Structural Depth Rule.** Elevation must explain whether an element is mounted, manipulable, recessed, or emitting light. It is never ambient decoration.

## Shapes

The system uses injection-moulded curves rather than sharp sheet-metal geometry. Compact primitives use tight corners, controls use medium corners, mounted modules use broader corners, and major housings use the largest radius. Pills are limited to LEDs, stamps, status, and compact metadata.

Panels may use subtle corner fasteners to imply mounting, but mechanical details remain sparse. Repeating every detail on every surface produces noise and weakens the hierarchy.

**The Mounted-Part Rule.** A curve, border, groove, or fastener must clarify the physical role of the element it belongs to.

## Components

### Buttons

- **Shape:** Stable 48px minimum targets with medium mechanical corners.
- **Primary:** Signal red with white labeling and red-tinted paired shadows.
- **Secondary:** Chassis material with instrument ink and structural paired shadows.
- **Hover / Focus / Press:** Hover rises slightly, focus receives an explicit signal-red outline, and press moves down while reversing the shadow inward.
- **Ghost:** Transparent and quiet; reserved for secondary commands that do not need physical prominence.

### Chips

- **Style:** Recessed pill stamps with mono labels and a visible border.
- **State:** A text label accompanies each LED or tone change; color alone never conveys status.

### Cards / Containers

- **Corner Style:** Mounted modules use the panel radius; compact device cards use the module radius.
- **Background:** Chassis or raised-panel material, with dark technical material reserved for logs and displays.
- **Shadow Strategy:** Outer shadow pairs mount a surface; inset pairs make wells and selected states.
- **Internal Padding:** Compact but sufficient for scanning, normally one large spacing unit.

### Inputs / Fields

- **Style:** Recessed chassis wells with mono values and conventional input affordances.
- **Focus:** A visible signal-red outline outside the well.
- **Error / Disabled:** Explain the blocking reason in text; reduce contrast for disabled state without erasing the label.

### Scroll Containers

- **Default:** Any application-owned internal region that can overflow, including a list, log, table, dialog pane, or inspector, uses `ScrollArea` from `@/components/ui/scroll-area`. It wraps the established `simplebar-react` track and thumb so scrolling remains visually consistent with the instrument panel.
- **Behavior:** Tracks auto-hide until hover, keyboard focus, or scrolling; content retains browser scrolling semantics and uses a visible focus path through its interactive children. Long content scrolls inside a bounded region rather than forcing an unexpected page-level scroll.
- **Exceptions:** Do not wrap the page or `body`. Browser- or system-owned controls, such as textareas, native selects, file choosers, and third-party popups, retain their platform scrolling. A region that is intentionally clipped or always fits does not need a scroll container.

### Navigation

Navigation is a set of physical keys rather than a generic sidebar. Each key combines a familiar Lucide icon, a direct label, and compact technical context. The active key is recessed and signal-red; hover lift never changes the layout.

### Status and Telemetry

LEDs use emitted glow, labels use mono typography, and large measurements dominate only inside the relevant runtime module. Nominal, warning, offline, mock, and live states keep explicit textual identity.

## Do's and Don'ts

### Do:

- **Do** preserve the top-left lighting model across every raised and recessed surface.
- **Do** keep standard buttons, inputs, switches, segmented controls, and destructive actions recognizable.
- **Do** show transport, lease, capability, and safety boundaries at the point of action.
- **Do** use mechanical detail selectively to explain mounting, state, or interaction.
- **Do** preserve responsive target sizes and visible keyboard focus.
- **Do** use the shared `ScrollArea` for application-owned internal scrolling.

### Don't:

- **Don't** present mock or stale telemetry as live hardware state.
- **Don't** turn the Web App into a marketing landing page, fleet dashboard, or generic SaaS settings shell.
- **Don't** use signal red as broad decoration or use status green/yellow for ordinary interaction.
- **Don't** nest decorative cards or give every region equal elevation.
- **Don't** introduce glass surfaces, arbitrary gradients, inconsistent lighting, or ornamental mechanical clutter.
- **Don't** add raw `overflow: auto` scrolling to an application-owned panel or dialog pane when `ScrollArea` is applicable.

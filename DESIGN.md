---
name: Codex Voice
description: Push-to-talk voice dictation for Linux
colors:
  accent: "#10A37F"
  surface: "#0F0F0F"
  surface-raised: "#1A1A1A"
  text: "#FAFAFA"
  text-muted: "#8E8E8E"
  border: "#2A2A2A"
  danger: "#FF6B6B"
typography:
  heading:
    fontFamily: "Inter, Ubuntu Sans, sans-serif"
    fontSize: "1.5rem"
    fontWeight: 600
    lineHeight: 1.2
  body:
    fontFamily: "Inter, Ubuntu Sans, sans-serif"
    fontSize: "1rem"
    fontWeight: 400
    lineHeight: 1.5
  label:
    fontFamily: "Inter, Ubuntu Sans, sans-serif"
    fontSize: "0.875rem"
    fontWeight: 400
    lineHeight: 1.45
rounded:
  sm: "8px"
  md: "12px"
  pill: "999px"
spacing:
  xs: "4px"
  sm: "8px"
  md: "12px"
  lg: "16px"
  xl: "24px"
components:
  button:
    backgroundColor: "{colors.surface-raised}"
    textColor: "{colors.text}"
    rounded: "{rounded.sm}"
    padding: "8px 12px"
  button-danger:
    backgroundColor: "{colors.surface-raised}"
    textColor: "{colors.danger}"
    rounded: "{rounded.sm}"
    padding: "8px 12px"
  card:
    backgroundColor: "{colors.surface-raised}"
    textColor: "{colors.text}"
    rounded: "{rounded.md}"
    padding: "16px"
  input:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.text}"
    rounded: "{rounded.sm}"
    padding: "8px 10px"
  pill:
    backgroundColor: "#0F0F0F"
    textColor: "{colors.text}"
    rounded: "{rounded.pill}"
    padding: "6px 8px"
---

# Design System: Codex Voice

## 1. Overview

**Creative North Star: "The Quiet Intercom"**

Codex Voice is a system utility, not an application. The design system reflects that: voice goes in, text comes out, and the chrome between is almost nothing. Every surface should feel like it was compiled alongside GNOME Shell — present, capable, never demanding attention. The accent color appears sparingly, like a status indicator on a well-made piece of hardware. The rest is neutral, precise, and quiet.

The palette follows OpenAI's brand language: near-black surfaces (`#0F0F0F`), off-white text (`#FAFAFA`), and a single signature teal-green accent (`#10A37F`) — the same green on the ChatGPT send button and streaming-response indicators. In a market full of neon-purple AI brands, this near-monochrome with one chromatic voice reads as the steady incumbent. The system is dark-first; light mode inverts the neutrals and keeps the same accent.

This system explicitly rejects bloated Electron settings panels (Discord, Spotify) with nested tabs, setup wizards, and unnecessary chrome. It rejects playful AI app aesthetics — gradient mascots, bouncy animations, rounded-everything. It rejects legacy dense settings dialogs with no visual hierarchy. The pill overlay rejects anything that competes with the user's focused window: no large surfaces, no chromatic accents, and no animation beyond the live waveform.

**Key Characteristics:**
- Dark-first, near-monochrome palette with a single teal-green accent
- Flat surfaces; depth conveyed by tonal layering, not shadows
- 8px radius for controls, 12px for containers, full-pill for the overlay
- Inter / Ubuntu Sans — one family, multiple weights, no display pairing
- Settings apply instantly; every change shows visible feedback
- The overlay pill is the signature component — it must feel native to GNOME

## 2. Colors

A near-monochrome palette with one chromatic voice. OpenAI's restraint is the doctrine: black, off-white, and a single teal-green that earns its visibility through scarcity.

### Primary
- **ChatGPT Green** (`#10A37F`): The single accent. Used on the shortcut-capture glow, checkbox accents, and the SVG mic icon. Never used as a large fill or a background. Its rarity is the point.

### Neutral
- **OpenAI Black** (`#0F0F0F`): The base surface in dark mode — the settings window background and the overlay pill background. A true near-black with no tint, matching OpenAI's brand black.
- **Graphite** (`#1A1A1A`): Raised surface in dark mode — settings cards, buttons, and inputs sit one step above the base. The tonal step is small (≈12% lighter) to keep the system flat.
- **Off White** (`#FAFAFA`): Primary text in dark mode, and the base surface in light mode. Warmth-free, neutral, high-contrast against OpenAI Black.
- **Slate Mist** (`#8E8E8E`): Muted text for descriptions, hints, and status lines. Must maintain 4.5:1 contrast against the surface — never go lighter than `#8E8E8E` on `#0F0F0F`.
- **Hairline** (`#2A2A2A`): Borders on cards, buttons, and inputs in dark mode. Subtle enough to define edges without creating visual noise. The border is a hairline, not a feature.

### Tertiary
- **Coral Alert** (`#FF6B6B`): Danger and destructive actions only — the reset button border and text, error alert borders. Never used as a fill. In light mode, deepens to `#D03A3A`.

### Light Mode
Light mode inverts the neutral ramp: surface becomes Off White (`#FAFAFA`), raised surface becomes Pure White (`#FFFFFF`), text becomes OpenAI Black (`#0F0F0F`), muted text becomes Stone (`#6B6B6B`), borders become Silver Line (`#E5E5E5`). The accent stays `#10A37F` — the teal-green reads well on both backgrounds. Danger deepens to `#D03A3A` for contrast.

### Named Rules
**The One Voice Rule.** ChatGPT Green appears on ≤10% of any given screen. It marks active state and recording indicators — nothing else. If two green elements are visible simultaneously, one is redundant.

**The No-Tint Rule.** Neutrals are pure. No green tinting in the blacks, whites, or grays. The accent carries all the chroma; the neutrals carry none. Tinting the neutrals toward the accent hue is the AI monoculture move — resist it.

## 3. Typography

**Display Font:** Inter (with Ubuntu Sans fallback)
**Body Font:** Inter (with Ubuntu Sans fallback)
**Label/Mono Font:** None distinct — Inter at smaller sizes serves all roles.

**Character:** A single geometric-grotesque family used across all roles. No serif pairing, no mono accent. The hierarchy comes from size and weight, not from family contrast. This is the GNOME convention — one system font, used well.

### Hierarchy
- **Heading** (600, 1.5rem / 24px, 1.2 line-height): The settings app title. Only one per screen.
- **Section Title** (400, 1rem / 16px, 1.4 line-height): Card headings — "General", "Shortcut", "Appearance", "Transcription language". Weight is regular; size and spacing carry the hierarchy.
- **Body** (400, 1rem / 16px, 1.5 line-height): Labels, button text, select options. Cap line length at 65–75ch where applicable.
- **Label / Muted** (400, 0.875rem / 14px, 1.45 line-height): Descriptions, hints, status lines, override notices. Uses `--cv-text-muted` color.

### Named Rules
**The One Family Rule.** Inter everywhere. Ubuntu Sans is the system fallback on GNOME — it shares Inter's grotesque geometry, so the fallback is invisible. Never introduce a second family for "personality"; the personality is in the restraint.

**The No-Eyebrow Rule.** No uppercase tracked eyebrows above section headings. Section titles are sentence-case at body size. The hierarchy comes from spacing and weight, not from decorative kickers.

## 4. Elevation

Flat by default. Depth is conveyed by tonal layering — the surface (`#0F0F0F`) and raised surface (`#1A1A1A`) create a 12% lightness step that defines cards and controls without any shadow. Borders (`#2A2A2A`) reinforce edges where tonal contrast alone is insufficient.

The only shadow-like effect in the system is the accent glow on the shortcut-capture button: `box-shadow: 0 0 0 4px color-mix(in srgb, #10A37F, transparent 70%)`. This is a state effect (the button is actively listening for input), not an elevation tool. It never appears on resting elements.

The modal overlay uses a backdrop dim (`rgba(0, 0, 0, 0.45)`) to separate itself from the content beneath — tonal layering at the page level, not a shadow on the dialog.

### Named Rules
**The Flat-By-Default Rule.** No element casts a shadow at rest. Depth is tonal: surface, raised surface, and border. Shadows are reserved for state (the capture glow) and are always accent-colored, never gray.

## 5. Components

### Buttons
- **Shape:** Gently curved edges (8px radius)
- **Primary:** Raised surface background (`#1A1A1A`), text color (`#FAFAFA`), 1px Hairline border, 8px 12px padding. Same style for all default buttons — there is no "primary fill" button in this system.
- **Danger:** Same shape and background, but border and text in Coral Alert (`#FF6B6B`). Never filled with red — the danger is signaled by color, not by a saturated background.
- **Hover / Focus:** Inherit browser default focus ring. No custom hover background change — the border and cursor are sufficient.
- **Shortcut Capture:** When actively capturing, pulses with an accent-colored box-shadow glow (4px ring, 70% transparent). This is the only animated button state.

### Cards / Settings Sections
- **Corner Style:** Rounded containers (12px radius)
- **Background:** Raised surface (`#1A1A1A`)
- **Shadow Strategy:** None. Flat — see Elevation.
- **Border:** 1px Hairline (`#2A2A2A`)
- **Internal Padding:** 16px
- **Structure:** One `<h2>` section title, then content. No nested cards. No card-within-card. Ever.

### Inputs / Fields
- **Style:** 1px Hairline border, surface background (`#0F0F0F` — one step below the card), 8px radius, 8px 10px padding
- **Focus:** Inherit browser default focus ring. No custom glow.

### Pills (Signature Component)
The overlay pill is the product's defining visual element. It appears during recording and transcription, then vanishes.
- **Shape:** Compact full pill (999px radius, 108px × 40px in GTK)
- **Background:** Fixed OpenAI Black (`#0F0F0F`), fully opaque in every system theme
- **Border:** 1px `rgba(250, 250, 250, 0.32)` — visible enough to define the surface, quiet enough to recede.
- **Content:** A slightly left-biased nine-bar off-white live waveform and a compact off-white × control, inset 8px from the trailing edge; it matches the settings preview exactly.
- **Transcribing State:** Waveform is replaced by a compact off-white loading spinner; the × control remains visible and active

### Modal
- **Backdrop:** `position: fixed`, `inset: 0`, `rgba(0, 0, 0, 0.45)` dim, `place-items: center`
- **Content:** A settings card (12px radius, raised surface) at `max-width: 24rem`

### Navigation
The settings app uses exactly two top-level tabs: General and Transcriptions. Tabs use a quiet full-width line treatment and accessible keyboard navigation. No sidebar, breadcrumbs, nested tabs, or deeper navigation.

## 6. Do's and Don'ts

### Do:
- **Do** use ChatGPT Green (`#10A37F`) only for active state and controls in the settings app. It appears on ≤10% of any screen.
- **Do** convey depth with tonal layering (surface → raised surface) and hairline borders, never with shadows.
- **Do** apply settings instantly with visible feedback — the fixed pill preview, the waveform, the checkbox state. No "Save" button.
- **Do** use 8px radius for controls and 12px for containers. Full-pill (999px) is reserved for the overlay pill and pill-preview elements.
- **Do** respect `prefers-color-scheme` in the settings app. The overlay pill stays fixed black and white in every system theme.
- **Do** keep the settings app to one scrollable column at `max-width: 768px`. No tabs, no sidebar.

### Don't:
- **Don't** use `border-left` or `border-right` greater than 1px as a colored accent stripe. The current `.override` class (`border-left: 3px solid var(--cv-accent)`) violates this — rewrite it with a full border, a background tint, or a leading icon.
- **Don't** use ChatGPT Green as a large fill, gradient, or background. It is an indicator color, not a surface color.
- **Don't** pair `border: 1px solid X` with `box-shadow: 0 Npx Mpx` (M ≥ 16px) on the same element. Pick a border OR a shadow, never both as decoration.
- **Don't** use border-radius greater than 12px on cards or inputs. 24px, 32px, 40px on a container is the over-rounding tell.
- **Don't** add tabs beyond the two top-level General and Transcriptions views, setup wizards, onboarding tours, or empty-state illustrations. It is a system preference pane, not an application.
- **Don't** tint neutrals toward green or any other hue. The blacks, whites, and grays are pure — the accent carries all chroma.
- **Don't** use gradient text (`background-clip: text` + gradient). Emphasis comes from weight or size, not from color effects.
- **Don't** use uppercase tracked eyebrows above section headings. Section titles are sentence-case at body size.
- **Don't** animate layout properties (width, height, padding). Use transform and opacity only. The waveform animates `transform: scaleY()`, not height.

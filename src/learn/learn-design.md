# learn.html design pattern

A reusable template for turning a single self-contained HTML file into a self-documenting, learn-by-poking-at-it reference. First built for [learn.html](learn.html) (an overview tour of the major browser Web APIs); reuse this pattern for the next topic rather than reinventing layout each time.

This is the same visual/interaction skeleton as the shrine pieces (see `src/0004/0004-rune_d.html`), generalised from "explain a piece of generative music" to "explain any topic with live, pokeable examples."

## The three-column shape

```
[ visual aid ]   [ interactive content ]   [ explainer ]
   240px               flexible                260px
   left                middle                  right, sticky
```

- **Left — visual aid.** A small canvas (240×240) showing a *map* of the topic — categories/concepts as nodes, related by lines to a central hub. It's decorative but not empty calories: hovering a card on the highlights the matching node, and clicking a node filters the content to that category. It gives the reader a spatial sense of "how many kinds of thing am I looking at, and which one am I in right now" before they've read a word.
- **Middle — the actual content.** A stack of collapsible cards (`.field`), one per concept, grouped under category headings. Each card holds a real, working demo — not a description of what the thing does, an actual instance of the thing doing it. This is the part worth the build time; the other two columns exist to support it.
- **Right — explainer.** `position:sticky`, updates on `mouseover` of anything with `data-explain="key"`. Shows a short title, a code-shaped snippet of the actual API calls involved, 1-3 sentences of prose, and — where cheap to compute — a "Right now: ..." line reading real live state out of the page (not a canned example value).

## Why hover-to-explain, not always-visible captions

Putting the explanation in a fixed side panel that *changes* on hover, rather than inline under each demo, does two things:
1. Keeps the middle column dense — cards can stay small and scannable because their explanation lives elsewhere.
2. Turns "reading" into "pointing" — the reader explores by moving the mouse across things that interest them, and the explainer becomes a running commentary rather than a wall of text they have to read top-to-bottom regardless of what they care about.

Two hover zones per card is normally enough: the card's header (`{id}-summary` — what is this API, why does it exist, one link to MDN) and its demo body (`{id}-code` — which specific methods/objects are running right now, with a live readout).

## Accordion, not scroll-forever

All cards start `.collapsed` (title bar only). Opening one collapses every other — bounded screen height no matter how many concepts the page covers, and it forces genuine lazy-init (see below) rather than everything running at once.

```js
function setCollapsed(item, collapsed){
  // ...toggle classes/arrow...
  if(!collapsed && !item.inited){ item.inited = true; item.init(el, item.state); }
  if(collapsed && item.inited && item.cleanup){ item.cleanup(item.state); }
}
```

## Data-driven cards, not hand-written HTML per concept

Every concept is one object in an array (`APIS` in learn.html) with the same shape: `id`, category, a one-line tag, a demo-HTML string, an `init(root, state)` that wires up real event listeners scoped to that card's root, an optional `cleanup(state)`, and `explainSummary()`/`explainCode()` functions that return `{title, html}`. Rendering, the accordion, the explainer wiring, and the category filter are all generic code that reads this array — adding a new concept later means adding one object, not touching the scaffolding.

This mirrors the `INSTRUMENTS` array pattern from the shrine pieces: one source-of-truth array, generic render/wire code underneath it.

## Lazy init + cleanup — required, not optional

A page with 15-20 live demos cannot afford to start every oscillator, worker, websocket, and geolocation watch on page load. Two rules:
- **Init on first expand**, not on page load — `item.inited` guards it.
- **Cleanup on collapse** — stop audio nodes, `worker.terminate()`, `ws.close()`, `channel.close()`, `cancelAnimationFrame()`, `IntersectionObserver.disconnect()`, remove any `window`-level listener the card added. The accordion already guarantees at most one card is open, so this keeps at most one demo's resources alive at a time.

Wrap both `init` and `cleanup` calls in `try/catch` at the call site (not inside every demo) so one misbehaving demo can't take down the whole page.

## Errors are part of the lesson, not something to hide

Several real browser APIs behave differently depending on context (secure vs. insecure origin, permission granted vs. denied, network reachable vs. not). Don't paper over this — catch the rejection/error and print *what actually happened* into the demo's output box. "clipboard read blocked: NotAllowedError" teaches the permission model better than a demo that silently no-ops. Where a whole category of API needs a secure context (Clipboard, Geolocation, Notifications, IndexedDB in some browsers), say so once in a small banner near the top rather than in every card.

## Category color-coding ties the whole page together

Pick one color per category up front, reuse it everywhere that category appears: the card's left border and `--flash-color`, the category-map node, the filter pill's dot, the category heading text. The reader learns the color coding once, subconsciously, and it does the grouping work that would otherwise need more words.

## Live readouts in the explainer

Where a value is already sitting in `state` (or trivially readable — `localStorage.getItem`, `Notification.permission`, `history.length`), read it fresh every time the explainer function runs and print it with a distinct visual treatment (`.tk-live`, a bright inline chip). Static prose describes the mechanism; the live value proves it's not a screenshot.

## Base visual language (shared with the shrine pieces)

- Dark, near-black background (`--bg:#101012`), monospace throughout (`ui-monospace, SFMono-Regular, Menlo, Consolas, monospace`), small type (9-11px) — a dense, technical, "instrument panel" feel rather than a marketing page.
- Panels (`--panel:#17171a`) with a 2px border that picks up the category/instrument color, sitting on a plain dark canvas.
- Uppercase, letter-spaced, dim labels (`.flabel`, `.cat-heading`) contrasted against normal-weight bright content — visually separates "this is a label" from "this is the thing."
- No external dependencies, no build step, no CDN — one `.html` file, opens by double-click. Keep it that way; it's what makes these genuinely disposable/forkable as learning artifacts.

## When to reach for this pattern again

Good fit: any topic that's naturally a *set of related things* (APIs, CSS properties, algorithms, music-theory concepts, statistical distributions) where "show me one working example of each, and let me poke at it" beats a linear article. Less good fit: a single deep topic with one continuous narrative — that's closer to the shrine pieces' own pattern (one system, many knobs into it) than to this one (many small systems, one knob each).

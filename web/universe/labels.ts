// Names in the sky. These are DOM elements so a screen reader skips them for the list and any script or
// any length of name just works. There are never more than 14, because each one costs on every frame.
import { formatWeight, topLabelIds } from './scene-data';
import type { Planet, Universe } from '../shared/types';

/** Where a Planet is on screen, in pixels from the top left of the sky. */
export interface OnScreen {
  x: number;
  y: number;
  /** How large it looks, in pixels. */
  radius: number;
}

export interface LabelBox extends OnScreen {
  width: number;
  height: number;
}

export interface Placed {
  x: number;
  y: number;
}

export interface Rect extends Placed {
  width: number;
  height: number;
}

const GAP = 6;

const overlap = (a: Rect, b: Rect) => a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height;

/**
 * Where each label goes, or null for one that would land on an earlier label, on `reserved`, or fall off
 * the sky. Earlier means more important, so a pile of Planets names only the largest. A label sits up and
 * to the right of its Planet, and flips to the left near the sky's right edge.
 */
export function placeLabels(boxes: (LabelBox | null)[], width: number, height: number, reserved: Rect[] = []): (Placed | null)[] {
  const taken = [...reserved];
  return boxes.map((box) => {
    if (!box) return null;
    const lean = box.radius * Math.SQRT1_2 + GAP;
    const flip = box.x + lean + box.width > width;
    const x = flip ? box.x - lean - box.width : box.x + lean;
    const y = box.y - lean - box.height / 2;
    const off = x < 0 || y < 0 || x + box.width > width || y + box.height > height;
    const here = { x, y, width: box.width, height: box.height };
    if (off || taken.some((t) => overlap(here, t))) return null;
    taken.push(here);
    return { x, y };
  });
}

/**
 * Where the hover tooltip goes. A name can be left out, but someone asked for this one, so it always gets a
 * place in the sky, clear of `avoid` if there is any room for that. First beside its Planet, flipping away
 * from an edge. Failing that, as near the Planet as it fits, beside whatever is in the way.
 */
export function placeTooltip(box: LabelBox, width: number, height: number, avoid: Rect[]): Placed {
  const lean = box.radius * Math.SQRT1_2 + GAP;
  const inSky = ({ x, y }: Placed): Placed => ({
    x: Math.max(0, Math.min(x, width - box.width)),
    y: Math.max(0, Math.min(y, height - box.height)),
  });
  const clear = (spot: Placed) => !avoid.some((r) => overlap({ ...spot, width: box.width, height: box.height }, r));

  const around = [box.y - lean - box.height / 2, box.y + lean - box.height / 2].flatMap((y) =>
    [box.x + lean, box.x - lean - box.width].map((x) => ({ x, y })),
  );
  const beside = around.find((spot) => clear(spot) && inSky(spot).x === spot.x && inSky(spot).y === spot.y);
  if (beside) return beside;

  const { x, y } = inSky(around[0]);
  const past = avoid.flatMap((r) => [
    { x, y: r.y + r.height + GAP },
    { x, y: r.y - box.height - GAP },
    { x: r.x + r.width + GAP, y },
    { x: r.x - box.width - GAP, y },
  ]);
  const away = (spot: Placed) => Math.hypot(spot.x + box.width / 2 - box.x, spot.y + box.height / 2 - box.y);
  const [nearest] = [...around, ...past].map(inSky).filter(clear).sort((a, b) => away(a) - away(b));
  return nearest ?? { x, y };
}

/** The boxes UniverseView paints over the sky, in the sky's own pixels. */
function coversOf(sky: HTMLElement): Rect[] {
  const { left, top } = sky.getBoundingClientRect();
  return [...sky.parentElement!.querySelectorAll('[data-covers-sky]')].map((node) => {
    const r = node.getBoundingClientRect();
    return { x: r.left - left, y: r.top - top, width: r.width, height: r.height };
  });
}

interface Label {
  element: HTMLElement;
  planet?: Planet;
  width: number;
  height: number;
  x: number;
  y: number;
  shown: boolean;
}

function makeLabel(parent: HTMLElement, className: string): Label {
  const element = document.createElement('div');
  element.className = className;
  element.dir = 'auto';
  element.hidden = true;
  parent.append(element);
  return { element, width: 0, height: 0, x: NaN, y: NaN, shown: false };
}

function fill(label: Label, planet: Planet | undefined, lines: string[] = planet ? [planet.name] : []) {
  label.planet = planet;
  label.element.replaceChildren(
    ...lines.map((line, i) => {
      const part = document.createElement('span');
      part.textContent = line;
      part.className = i ? 'detail' : 'name';
      part.dir = 'auto';
      return part;
    }),
  );
  label.width = label.height = 0;
}

export function createLabels(parent: HTMLElement) {
  const layer = document.createElement('div');
  layer.className = 'labels';
  layer.setAttribute('aria-hidden', 'true');
  parent.append(layer);

  const tooltip = makeLabel(layer, 'label tooltip');
  const chosen = makeLabel(layer, 'label chosen');
  let top: Label[] = [];
  let universe: Universe | undefined;

  return {
    setUniverse(next: Universe) {
      universe = next;
      for (const label of top) label.element.remove();
      const planets = next.planets;
      top = topLabelIds(planets).map((id) => {
        const label = makeLabel(layer, 'label');
        fill(label, planets[id]);
        return label;
      });
      fill(tooltip, undefined);
      fill(chosen, undefined);
    },

    setChosen(planet: Planet | undefined) {
      fill(chosen, planet);
    },

    /** The hover tooltip: the name again, with how much and which Galaxy. */
    setHovered(planet: Planet | undefined) {
      const galaxy = planet && universe?.galaxies[planet.galaxy]?.name;
      fill(tooltip, planet, planet && [planet.name, `${formatWeight(planet)}${galaxy ? `, ${galaxy} Galaxy` : ''}`]);
    },

    /** Moves every label. `project` gives null for a Planet behind the camera. */
    update(project: (planet: Planet) => OnScreen | null, width: number, height: number) {
      // In order of importance, because earlier labels win. A Planet with its own tooltip needs no other label.
      const order = [tooltip, chosen, ...top];
      const seen = new Set<Planet>();
      const live = order.filter((label) => {
        if (!label.planet || seen.has(label.planet)) return false;
        seen.add(label.planet);
        return true;
      });

      for (const label of live) {
        if (label.width) continue;
        // Reading a size needs it on the page, and this only happens when the words change.
        label.element.hidden = false;
        label.shown = true;
        label.width = label.element.offsetWidth;
        label.height = label.element.offsetHeight;
      }

      // A Planet can fill the view with its middle behind the camera, and it was still hovered.
      const anchor = tooltip.planet && (project(tooltip.planet) ?? { x: width / 2, y: height / 2, radius: 0 });
      const size = { width: tooltip.width, height: tooltip.height };
      const tip = anchor ? placeTooltip({ ...anchor, ...size }, width, height, coversOf(parent)) : null;

      const names = live.filter((label) => label !== tooltip);
      const placed = placeLabels(
        names.map((label) => {
          const at = project(label.planet!);
          return at && { ...at, width: label.width, height: label.height };
        }),
        width,
        height,
        tip ? [{ ...tip, ...size }] : [],
      );

      for (const label of order) {
        const index = names.indexOf(label);
        const place = label === tooltip ? tip : index < 0 ? null : placed[index];
        const shown = place !== null;
        if (shown !== label.shown) {
          label.element.hidden = !shown;
          label.shown = shown;
        }
        if (place && (place.x !== label.x || place.y !== label.y)) {
          label.element.style.transform = `translate(${Math.round(place.x)}px, ${Math.round(place.y)}px)`;
          label.x = place.x;
          label.y = place.y;
        }
      }
    },

    destroy() {
      layer.remove();
    },
  };
}

export type Labels = ReturnType<typeof createLabels>;

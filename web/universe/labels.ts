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

const GAP = 6;

/**
 * Where each label goes, or null for one that would land on an earlier label or fall off the sky. Earlier
 * means more important, so a pile of Planets names only the largest. A label sits up and to the right of
 * its Planet, and flips to the left near the sky's right edge.
 */
export function placeLabels(boxes: (LabelBox | null)[], width: number, height: number): (Placed | null)[] {
  const taken: { x: number; y: number; width: number; height: number }[] = [];
  return boxes.map((box) => {
    if (!box) return null;
    const lean = box.radius * Math.SQRT1_2 + GAP;
    const flip = box.x + lean + box.width > width;
    const x = flip ? box.x - lean - box.width : box.x + lean;
    const y = box.y - lean - box.height / 2;
    const off = x < 0 || y < 0 || x + box.width > width || y + box.height > height;
    const hit = taken.some((t) => x < t.x + t.width && t.x < x + box.width && y < t.y + t.height && t.y < y + box.height);
    if (off || hit) return null;
    taken.push({ x, y, width: box.width, height: box.height });
    return { x, y };
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

      const placed = placeLabels(
        live.map((label) => {
          const at = project(label.planet!);
          return at && { ...at, width: label.width, height: label.height };
        }),
        width,
        height,
      );

      for (const label of order) {
        const index = live.indexOf(label);
        const place = index < 0 ? null : placed[index];
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

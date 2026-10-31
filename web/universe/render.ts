// The three.js scene for a Universe. Rust decides where everything goes, and this only draws it.
import {
  AdditiveBlending,
  BackSide,
  BufferGeometry,
  Color,
  DoubleSide,
  Euler,
  Float32BufferAttribute,
  Group,
  InstancedBufferAttribute,
  InstancedMesh,
  LineSegments,
  MathUtils,
  Matrix4,
  Mesh,
  PerspectiveCamera,
  PlaneGeometry,
  Points,
  Quaternion,
  Raycaster,
  RingGeometry,
  Scene,
  ShaderMaterial,
  SphereGeometry,
  Vector2,
  Vector3,
  WebGLRenderer,
} from 'three';
import { OrbitControls } from 'three/addons/controls/OrbitControls.js';
import { createLabels } from './labels';
import { galaxyColor, linkOpacity, pickPlanet, planetSeed } from './scene-data';
import * as glsl from './shaders';
import type { Planet, Universe } from '../shared/types';

export interface UniverseHandle {
  /** Draws another Universe in the same frame. `frame: false` leaves the camera where it is, if it can still see anything. */
  setUniverse(universe: Universe, options?: { frame?: boolean }): void;
  /** Puts the brass scale and a name on a Planet, or takes them off with null. */
  select(planetId: number | null): void;
  flyTo(planetId: number): void;
  /** Flies back out to see every Planet. */
  frameAll(): void;
  /** `callback` gets the `id` of the Planet someone clicked. */
  onSelect(callback: (planetId: number) => void): void;
  /** `callback` gets true when the browser takes the drawing context away, and false if it gives it back. */
  onContextChange(callback: (lost: boolean) => void): void;
  destroy(): void;
}

/** Thrown where this computer cannot start WebGL, so a screen can tell that apart from a bug. */
export class NoWebGL extends Error {}

const FIELD_OF_VIEW = 45;
const HALF_VIEW = MathUtils.degToRad(FIELD_OF_VIEW / 2);
const STARS = 4200;
/** A click that moves further than this, in pixels, was a drag. */
const CLICK_SLOP = 4;
/** A Planet this few pixels across can still be aimed at from this far outside it. */
const AIM_PIXELS = 6;
const FLIGHT_MS = 1100;
const ARRIVAL_MS = 2200;
/** One turn in about four minutes. */
const ROTATION_SPEED = 0.25;
const ARC_SEGMENTS = 14;
/** A lone Planet still looks like a Planet in a sky, not a wall. */
const SMALLEST_SKY = 11;
const IVORY: Rgb = [0.96, 0.92, 0.82];
const BRASS: Rgb = [0.84, 0.66, 0.33];
/** A ring's inner and outer edge, in Planet radii. */
const RING: [number, number] = [1.46, 1.74];

type Rgb = [number, number, number];

/** Fixed starting points, so the sky lands the same way each time in the app and on a Share Site. */
function random(seed: number) {
  let state = Math.floor(seed * 2147483646) + 1;
  return () => (state = (state * 16807) % 2147483647) / 2147483647;
}

const mix = (a: Rgb, b: Rgb, share: number) => a.map((channel, i) => channel + (b[i] - channel) * share) as Rgb;

function makeMaterials() {
  const distant = { value: new Vector2(1, 2) };
  const pixels = { value: 1 };
  const glow = { blending: AdditiveBlending, transparent: true, depthWrite: false };
  const material = (vertexShader: string, fragmentShader: string, rest: ConstructorParameters<typeof ShaderMaterial>[0] = {}) =>
    new ShaderMaterial({ vertexShader, fragmentShader, ...rest });
  return {
    distant,
    pixels,
    planet: material(glsl.planetVertex, glsl.planetFragment, { uniforms: { distant } }),
    ring: material(glsl.ringVertex, glsl.ringFragment, {
      uniforms: { distant, inner: { value: RING[0] }, outer: { value: RING[1] } },
      transparent: true,
      depthWrite: false,
      side: DoubleSide,
    }),
    glow: material(glsl.glowVertex, glsl.glowFragment, glow),
    line: material(glsl.lineVertex, glsl.lineFragment, { uniforms: { distant }, ...glow }),
    dustStar: material(glsl.dustStarVertex, glsl.starFragment, { uniforms: { pixels }, ...glow }),
    star: material(glsl.starVertex, glsl.starFragment, { uniforms: { pixels }, ...glow }),
    dome: material(glsl.domeVertex, glsl.domeFragment, { side: BackSide, depthWrite: false, depthTest: false }),
    reticle: material(glsl.reticleVertex, glsl.reticleFragment, {
      uniforms: { centre: { value: new Vector3() }, size: { value: 1 } },
      transparent: true,
      depthTest: false,
      depthWrite: false,
    }),
  };
}

type Materials = ReturnType<typeof makeMaterials>;

/** The river of stars and the sky behind it. It belongs to no Universe, so it stays put when one changes. */
function background(materials: Materials) {
  const sky = new Group();
  const dome = new Mesh(new SphereGeometry(1, 96, 64), materials.dome);
  dome.renderOrder = -2;
  dome.frustumCulled = false;
  sky.add(dome);

  const next = random(0.2024);
  const positions: number[] = [];
  const sizes: number[] = [];
  const tints: number[] = [];
  const band = new Vector3(0.28, 0.82, -0.5).normalize();
  const at = new Vector3();
  while (sizes.length < STARS) {
    const z = next() * 2 - 1;
    const angle = next() * Math.PI * 2;
    const around = Math.sqrt(1 - z * z);
    at.set(around * Math.cos(angle), z, around * Math.sin(angle));
    // Half of them gather along the same band as the sky's river of light.
    if (next() > 0.35 + 0.65 * Math.exp(-Math.pow(at.dot(band), 2) * 9)) continue;
    positions.push(at.x, at.y, at.z);
    const brightness = Math.pow(next(), 4);
    sizes.push(1 + brightness * 2.2);
    const warmth = next();
    const light = 0.28 + 0.72 * brightness;
    tints.push(light * (0.78 + warmth * 0.22), light * (0.82 + warmth * 0.08), light * (1 - warmth * 0.26));
  }
  const geometry = new BufferGeometry()
    .setAttribute('position', new Float32BufferAttribute(positions, 3))
    .setAttribute('size', new Float32BufferAttribute(sizes, 1))
    .setAttribute('tint', new Float32BufferAttribute(tints, 3));
  const stars = new Points(geometry, materials.star);
  stars.frustumCulled = false;
  sky.add(stars);
  return sky;
}

function surfaces(universe: Universe, materials: Materials, colors: Map<number, Rgb>) {
  const { planets } = universe;
  const mesh = new InstancedMesh(new SphereGeometry(1, 48, 32), materials.planet, planets.length);
  const rings = planets.filter((p) => p.source === 'youtube');
  const ringMesh = new InstancedMesh(new RingGeometry(RING[0], RING[1], 96, 1), materials.ring, rings.length);
  const seeds = new Float32Array(planets.length);
  const rock = new Float32Array(planets.length);
  const matrix = new Matrix4();
  const tilt = new Quaternion();
  const axis = new Vector3();
  const color = new Color();
  let ringAt = 0;
  planets.forEach((planet, i) => {
    const seed = planetSeed(planet.name);
    const next = random(seed);
    seeds[i] = seed;
    rock[i] = planet.source === 'youtube' ? 1 : 0;
    // A Planet's bands and craters turn with it, so each sits at its own angle.
    axis.set(next() - 0.5, next() - 0.5, next() - 0.5).normalize();
    tilt.setFromAxisAngle(axis, (next() - 0.5) * 1.1);
    const place = new Vector3(...planet.position);
    const scale = new Vector3().setScalar(planet.radius);
    matrix.compose(place, tilt, scale);
    mesh.setMatrixAt(i, matrix);
    mesh.setColorAt(i, color.setRGB(...colors.get(planet.galaxy)!));
    if (planet.source === 'youtube') {
      // A flat ring faces the viewer to begin with, so lay it well over and then lean it its own way.
      tilt.setFromEuler(new Euler(1.12 + next() * 0.32, 0, (next() - 0.5) * 1.2, 'ZXY'));
      matrix.compose(place, tilt, scale);
      ringMesh.setMatrixAt(ringAt, matrix);
      ringMesh.setColorAt(ringAt, color.setRGB(...colors.get(planet.galaxy)!));
      ringAt += 1;
    }
  });
  mesh.geometry.setAttribute('seed', new InstancedBufferAttribute(seeds, 1));
  mesh.geometry.setAttribute('rock', new InstancedBufferAttribute(rock, 1));
  // Nothing here is ever a good candidate to cull, and skipping it means no bounding sphere to keep right.
  mesh.frustumCulled = false;
  ringMesh.frustumCulled = false;
  ringMesh.renderOrder = 2;
  return rings.length ? [mesh, ringMesh] : [mesh];
}

/**
 * Soft light: a halo round each Planet, and a cloud of dust round each, so a Galaxy shows as one place. A cloud
 * is cheap to work out but covers a lot of pixels, and that is what limits the frame rate here, so one each.
 */
function glows(universe: Universe, materials: Materials, colors: Map<number, Rgb>) {
  const { planets } = universe;
  const mesh = new InstancedMesh(new PlaneGeometry(2, 2), materials.glow, planets.length * 2);
  const matrix = new Matrix4();
  const color = new Color();
  let at = 0;
  const add = (position: Rgb, size: number, tint: Rgb, amount: number) => {
    matrix.makeScale(size, size, size).setPosition(...position);
    mesh.setMatrixAt(at, matrix);
    mesh.setColorAt(at, color.setRGB(tint[0] * amount, tint[1] * amount, tint[2] * amount));
    at += 1;
  };

  // How far a Galaxy's Planets spread, so its cloud covers them all and no more.
  const reach = new Map<number, number>();
  for (const { id } of universe.galaxies) {
    const group = planets.filter((p) => p.galaxy === id);
    const middle = group.reduce((sum, p) => sum.add(new Vector3(...p.position)), new Vector3()).divideScalar(group.length);
    reach.set(id, Math.max(4, ...group.map((p) => middle.distanceTo(new Vector3(...p.position)) + p.radius)));
  }

  for (const planet of planets) {
    const tint = colors.get(planet.galaxy)!;
    add(planet.position, planet.radius * 2.5, mix(tint, IVORY, 0.12), planet.source === 'youtube' ? 0.22 : 0.3);
    const next = random(planetSeed(`${planet.name} dust`));
    const spread = reach.get(planet.galaxy)!;
    const place = planet.position.map((c) => c + (next() - 0.5) * spread * 0.7) as Rgb;
    add(place, spread * (0.72 + next() * 0.3), mix(tint, [0.2, 0.4, 0.55], 0.25 * next()), 0.1 + next() * 0.05);
  }
  mesh.frustumCulled = false;
  return mesh;
}

/** Tiny stars gathered round each Galaxy's Planets, which gives its cloud a grain and moves as you fly past. */
function dustStars(universe: Universe, materials: Materials, colors: Map<number, Rgb>) {
  const positions: number[] = [];
  const sizes: number[] = [];
  const tints: number[] = [];
  for (const planet of universe.planets) {
    const next = random(planetSeed(`${planet.name} stars`));
    const tint = mix(colors.get(planet.galaxy)!, IVORY, 0.45);
    for (let i = 0; i < 14; i += 1) {
      const spread = planet.radius * 2 + 7;
      positions.push(...planet.position.map((c) => c + (next() + next() - 1) * spread));
      const light = 0.25 + Math.pow(next(), 3) * 0.75;
      sizes.push(0.9 + light * 1.3);
      tints.push(...tint.map((channel) => channel * light));
    }
  }
  const stars = new Points(
    new BufferGeometry()
      .setAttribute('position', new Float32BufferAttribute(positions, 3))
      .setAttribute('size', new Float32BufferAttribute(sizes, 1))
      .setAttribute('tint', new Float32BufferAttribute(tints, 3)),
    materials.dustStar,
  );
  stars.frustumCulled = false;
  return stars;
}

/** Each Link is a thin arc away from the middle of the sky, like a path a thing could actually follow. */
function links(universe: Universe, materials: Materials, colors: Map<number, Rgb>, middle: Vector3) {
  const positions: number[] = [];
  const tints: number[] = [];
  const [a, b, between, outward, previous, here] = Array.from({ length: 6 }, () => new Vector3());
  for (const link of universe.links) {
    const from = universe.planets[link.a];
    const to = universe.planets[link.b];
    a.set(...from.position);
    b.set(...to.position);
    between.addVectors(a, b).multiplyScalar(0.5);
    outward.subVectors(between, middle);
    if (outward.lengthSq() < 1e-6) outward.set(0, 1, 0);
    outward.normalize().multiplyScalar(a.distanceTo(b) * 0.32).add(between);
    const opacity = linkOpacity(link.strength);
    const colorA = mix(colors.get(from.galaxy)!, IVORY, 0.35);
    const colorB = mix(colors.get(to.galaxy)!, IVORY, 0.35);
    const vertex = (t: number, point: Vector3) => {
      positions.push(point.x, point.y, point.z);
      // Brighter near the Planets and fainter between them, so a crowd of Links keeps its shape.
      tints.push(...mix(colorA, colorB, t), opacity * (0.5 + Math.abs(t - 0.5)));
    };
    previous.copy(a);
    for (let segment = 1; segment <= ARC_SEGMENTS; segment += 1) {
      const t = segment / ARC_SEGMENTS;
      const u = 1 - t;
      here.copy(a).multiplyScalar(u * u).addScaledVector(outward, 2 * u * t).addScaledVector(b, t * t);
      vertex((segment - 1) / ARC_SEGMENTS, previous);
      vertex(t, here);
      previous.copy(here);
    }
  }
  const lines = new LineSegments(
    new BufferGeometry()
      .setAttribute('position', new Float32BufferAttribute(positions, 3))
      .setAttribute('tint', new Float32BufferAttribute(tints, 4)),
    materials.line,
  );
  lines.frustumCulled = false;
  return lines;
}

/** The instrument around it all: three brass circles with a scale marked on them, as on an armillary sphere. */
function armillary(materials: Materials, middle: Vector3, radius: number) {
  const positions: number[] = [];
  const tints: number[] = [];
  const size = radius * 1.08;
  const add = (x: number, y: number, z: number, amount: number) => {
    positions.push(x, y, z);
    tints.push(...BRASS, amount);
  };
  const circle = (tiltX: number, tiltZ: number) => {
    const turn = new Matrix4().makeRotationX(tiltX).multiply(new Matrix4().makeRotationZ(tiltZ));
    const point = new Vector3();
    const on = (degrees: number, out: number) => {
      const angle = MathUtils.degToRad(degrees);
      return point.set(Math.cos(angle) * out, 0, Math.sin(angle) * out).applyMatrix4(turn).toArray() as Rgb;
    };
    for (let d = 0; d < 360; d += 2) {
      add(...on(d, size), 0.34);
      add(...on(d + 2, size), 0.34);
      if (d % 10 === 0) {
        const long = d % 30 === 0;
        add(...on(d, size), long ? 0.5 : 0.3);
        add(...on(d, size * (long ? 1.024 : 1.012)), long ? 0.5 : 0.3);
      }
    }
  };
  circle(0, 0);
  circle(0.41, 0.2);
  circle(1.25, -0.5);
  const rings = new LineSegments(
    new BufferGeometry()
      .setAttribute('position', new Float32BufferAttribute(positions, 3))
      .setAttribute('tint', new Float32BufferAttribute(tints, 4)),
    materials.line,
  );
  rings.position.copy(middle);
  rings.frustumCulled = false;
  return rings;
}

function drawing(universe: Universe, materials: Materials, middle: Vector3, radius: number) {
  const sky = new Group();
  if (!universe.planets.length) return sky;
  const colors = new Map(universe.galaxies.map((g) => [g.id, galaxyColor(g.hue)]));
  sky.add(
    armillary(materials, middle, radius),
    dustStars(universe, materials, colors),
    glows(universe, materials, colors),
    links(universe, materials, colors, middle),
    ...surfaces(universe, materials, colors),
  );
  return sky;
}

export function mountUniverse(container: HTMLElement, first: Universe): UniverseHandle {
  let renderer: WebGLRenderer;
  try {
    renderer = new WebGLRenderer({ antialias: true });
  } catch (e) {
    throw new NoWebGL(String(e));
  }
  renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
  const canvas = renderer.domElement;
  canvas.style.display = 'block';
  canvas.tabIndex = 0;
  canvas.setAttribute('role', 'img');
  canvas.setAttribute('aria-label', 'Your Universe, drawn in 3D. A list view of every Planet is available with the Show list button.');
  container.append(canvas);

  const materials = makeMaterials();
  materials.pixels.value = renderer.getPixelRatio();
  const labels = createLabels(container);

  const scene = new Scene();
  const backdrop = background(materials);
  scene.add(backdrop);
  const reticle = new Mesh(new PlaneGeometry(2, 2), materials.reticle);
  reticle.visible = false;
  reticle.frustumCulled = false;
  reticle.renderOrder = 3;
  scene.add(reticle);

  const camera = new PerspectiveCamera(FIELD_OF_VIEW);
  const controls = new OrbitControls(camera, canvas);
  controls.enableDamping = true;
  controls.dampingFactor = 0.09;
  controls.autoRotateSpeed = ROTATION_SPEED;
  controls.listenToKeyEvents(canvas);

  let universe = first;
  let current: Group | undefined;
  let chosen: Planet | undefined;
  let hovered: Planet | undefined;
  let width = 0;
  let height = 0;
  const middle = new Vector3();
  let radius = 1;
  // Once someone has touched the view, it stops turning by itself.
  let touched = false;
  // While the view is of the whole sky, with nobody having moved it since, a resize keeps it fitted.
  let framed = true;

  const calm = matchMedia('(prefers-reduced-motion: reduce)');
  /** Only draws when something changes, so a still Universe costs no battery. */
  let frame = 0;
  const wake = () => {
    frame ||= requestAnimationFrame(tick);
  };

  let flight:
    | { start: number; ms: number; from: Vector3; to: Vector3; fromDistance: number; toDistance: number; way: Vector3 }
    | undefined;

  function fly(now: number) {
    const f = flight!;
    const t = f.ms ? Math.min(Math.max((now - f.start) / f.ms, 0), 1) : 1;
    const k = t * t * t * (t * (t * 6 - 15) + 10);
    // By ratio, not difference, so the world grows at a steady rate as the camera comes in.
    const distance = f.fromDistance * Math.pow(f.toDistance / f.fromDistance, k);
    controls.target.lerpVectors(f.from, f.to, k);
    camera.position.copy(controls.target).addScaledVector(f.way, distance);
    if (t === 1) flight = undefined;
  }

  let lastTick = 0;
  function tick(now: number) {
    frame = 0;
    if (flight) fly(now);
    // Not while the list is showing, since nobody could see it turn.
    controls.autoRotate = !touched && !calm.matches && !flight && width > 0;
    // Seconds, so a slow computer turns the sky at the same speed. A long gap was a hidden tab, not a slow frame.
    controls.update(Math.min((now - lastTick) / 1000, 0.1));
    lastTick = now;
    draw();
    if (flight || controls.autoRotate) wake();
  }

  const at = new Vector3();
  function project(planet: Planet) {
    at.set(...planet.position);
    const distance = at.distanceTo(camera.position);
    at.project(camera);
    if (Math.abs(at.z) > 1) return null;
    return {
      x: ((at.x + 1) / 2) * width,
      y: ((1 - at.y) / 2) * height,
      radius: (planet.radius / (distance * Math.tan(HALF_VIEW))) * (height / 2),
    };
  }

  function draw() {
    if (!current) return;
    const distance = camera.position.distanceTo(controls.target);
    materials.distant.value.set(distance, distance + radius * 1.6);
    renderer.render(scene, camera);
    labels.update(project, width, height);
  }

  function fit() {
    const { clientWidth, clientHeight } = container;
    width = clientWidth;
    height = clientHeight;
    if (!width || !height) return;
    renderer.setSize(width, height);
    camera.aspect = width / height;
    camera.updateProjectionMatrix();
  }

  /** How far back to see a sphere of this size whichever way the window is shaped. */
  const distanceFor = (size: number) => size / Math.sin(HALF_VIEW) / Math.min(camera.aspect, 1);
  const homeDistance = () => distanceFor(radius) * 0.94;

  function flyToward(target: Vector3, distance: number, ms = FLIGHT_MS) {
    const way = camera.position.clone().sub(controls.target);
    const fromDistance = way.length();
    way.normalize();
    flight = {
      start: performance.now(),
      ms: calm.matches ? 0 : ms,
      from: controls.target.clone(),
      to: target.clone(),
      fromDistance,
      toDistance: distance,
      way,
    };
    wake();
  }

  function frameAll() {
    // The layout may have changed this frame, and the observer that says so comes too late for this flight.
    fit();
    framed = true;
    controls.minDistance = 0.5;
    flyToward(middle, homeDistance());
  }

  /** Straight to a view of everything from where the camera is looking, with no flight. */
  function jumpHome(back = 1) {
    controls.minDistance = 0.5;
    const way = camera.position.clone().sub(controls.target).normalize();
    controls.target.copy(middle);
    camera.position.copy(middle).addScaledVector(way, homeDistance() * back);
    controls.update();
  }

  function install(next: Universe) {
    const nextMiddle = new Vector3();
    next.planets.forEach((p) => nextMiddle.add(new Vector3(...p.position)));
    if (next.planets.length) nextMiddle.divideScalar(next.planets.length);
    const nextRadius = Math.max(SMALLEST_SKY, ...next.planets.map((p) => new Vector3(...p.position).distanceTo(nextMiddle) + p.radius));
    // First, so a Universe that cannot be drawn leaves the one on screen alone.
    const sky = drawing(next, materials, nextMiddle, nextRadius);
    release();
    current = sky;
    scene.add(sky);
    universe = next;
    middle.copy(nextMiddle);
    radius = nextRadius;
    chosen = undefined;
    setHovered(undefined);
    reticle.visible = false;
    labels.setUniverse(next);

    fit();
    const home = homeDistance();
    camera.near = 0.1;
    camera.far = home * 3 + radius * 8;
    camera.updateProjectionMatrix();
    controls.maxDistance = home * 2.5;
  }

  function setUniverse(next: Universe, { frame: reframe = true }: { frame?: boolean } = {}) {
    install(next);
    // Keeping the camera only makes sense if someone put it there and it can still see this sky.
    const lost = camera.position.distanceTo(middle) > controls.maxDistance || controls.target.distanceTo(middle) > radius;
    if (reframe || !touched || lost) frameAll();
    wake();
  }

  function release() {
    if (!current) return;
    scene.remove(current);
    current.traverse((object) => {
      (object as Mesh).geometry?.dispose();
      (object as InstancedMesh).dispose?.();
    });
  }

  const raycaster = new Raycaster();
  const pointer = new Vector2();
  function planetAt(e: { clientX: number; clientY: number }) {
    const box = canvas.getBoundingClientRect();
    pointer.set(((e.clientX - box.left) / box.width) * 2 - 1, -((e.clientY - box.top) / box.height) * 2 + 1);
    raycaster.setFromCamera(pointer, camera);
    const { origin, direction } = raycaster.ray;
    const perPixel = (2 * Math.tan(HALF_VIEW)) / box.height;
    return pickPlanet(universe.planets, origin.toArray(), direction.toArray(), perPixel * AIM_PIXELS);
  }

  const stopTurning = () => {
    touched = true;
    framed = false;
    flight = undefined;
  };
  controls.addEventListener('start', stopTurning);
  controls.addEventListener('change', wake);
  const onKey = (e: KeyboardEvent) => {
    if (e.key.startsWith('Arrow')) stopTurning();
  };
  canvas.addEventListener('keydown', onKey);

  let select: ((planetId: number) => void) | undefined;
  let downAt: { x: number; y: number } | undefined;
  // Only the main button, since the others pan the view.
  const onDown = (e: PointerEvent) => (downAt = e.button === 0 ? { x: e.clientX, y: e.clientY } : undefined);
  const onUp = (e: PointerEvent) => {
    const wasClick = downAt && Math.hypot(e.clientX - downAt.x, e.clientY - downAt.y) <= CLICK_SLOP;
    downAt = undefined;
    if (!wasClick) return;
    const planet = planetAt(e);
    if (planet) select?.(planet.id);
  };
  function setHovered(planet: Planet | undefined) {
    if (planet === hovered) return;
    hovered = planet;
    labels.setHovered(planet);
    canvas.style.cursor = planet ? 'pointer' : '';
    labels.update(project, width, height);
  }
  const onMove = (e: PointerEvent) => {
    // Nothing to hover with a finger, or while dragging.
    if (e.pointerType === 'touch' || e.buttons) return;
    setHovered(planetAt(e));
  };
  const onLeave = () => setHovered(undefined);
  canvas.addEventListener('pointerdown', onDown);
  canvas.addEventListener('pointerup', onUp);
  canvas.addEventListener('pointermove', onMove);
  canvas.addEventListener('pointerleave', onLeave);

  let contextChange: ((lost: boolean) => void) | undefined;
  const onContextLost = () => contextChange?.(true);
  // three.js restores its own state first, but this only draws when asked, so the sky stays empty until then.
  const onContextRestored = () => {
    wake();
    contextChange?.(false);
  };
  canvas.addEventListener('webglcontextlost', onContextLost);
  canvas.addEventListener('webglcontextrestored', onContextRestored);

  const resize = new ResizeObserver(() => {
    fit();
    // A view of everything fits the window it is in. That includes coming back from hidden.
    if (framed && !flight) jumpHome();
    // Now, in the same frame as the layout change, so the sky never shows blank.
    draw();
    wake();
  });
  resize.observe(container);

  function destroy() {
    cancelAnimationFrame(frame);
    resize.disconnect();
    canvas.removeEventListener('pointerdown', onDown);
    canvas.removeEventListener('pointerup', onUp);
    canvas.removeEventListener('pointermove', onMove);
    canvas.removeEventListener('pointerleave', onLeave);
    canvas.removeEventListener('keydown', onKey);
    canvas.removeEventListener('webglcontextlost', onContextLost);
    canvas.removeEventListener('webglcontextrestored', onContextRestored);
    controls.dispose();
    release();
    labels.destroy();
    backdrop.traverse((object) => (object as Mesh).geometry?.dispose());
    reticle.geometry.dispose();
    for (const material of Object.values(materials)) {
      if (material instanceof ShaderMaterial) material.dispose();
    }
    renderer.dispose();
    // A browser allows about 16 live contexts, and it only frees one when it gets round to it.
    renderer.forceContextLoss();
    canvas.remove();
  }

  // The way the camera first looks, a little above the middle of the sky.
  camera.position.set(0, 0.25, 1);
  try {
    install(first);
  } catch (e) {
    // Nothing else holds on to this context, so it has to go now or it stays alive until the page does.
    destroy();
    throw e;
  }
  // One long approach when the sky first opens, which is all the motion a person did not ask for.
  jumpHome(calm.matches ? 1 : 1.5);
  if (!calm.matches) flyToward(middle, homeDistance(), ARRIVAL_MS);
  wake();

  return {
    setUniverse,
    select(id) {
      chosen = id === null ? undefined : universe.planets[id];
      labels.setChosen(chosen);
      reticle.visible = Boolean(chosen);
      if (chosen) {
        materials.reticle.uniforms.centre.value.set(...chosen.position);
        materials.reticle.uniforms.size.value = chosen.radius * (chosen.source === 'youtube' ? RING[1] + 0.32 : 1.3);
      }
      wake();
    },
    flyTo(id) {
      const planet = universe.planets[id];
      if (!planet) return;
      touched = true;
      framed = false;
      controls.minDistance = planet.radius * 1.5;
      // A ring needs a little more room than a bare Planet does.
      flyToward(new Vector3(...planet.position), distanceFor(planet.radius) * (planet.source === 'youtube' ? 2.5 : 2.1));
    },
    frameAll,
    onSelect: (callback) => (select = callback),
    onContextChange: (callback) => (contextChange = callback),
    destroy,
  };
}

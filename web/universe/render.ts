// The three.js scene for a Universe. Rust decides where everything goes, and this only draws it.
import {
  AmbientLight,
  BufferGeometry,
  Color,
  DirectionalLight,
  Float32BufferAttribute,
  Group,
  InstancedMesh,
  LineBasicMaterial,
  LineSegments,
  Matrix4,
  MeshStandardMaterial,
  PerspectiveCamera,
  Points,
  PointsMaterial,
  Raycaster,
  Scene,
  SphereGeometry,
  Vector2,
  Vector3,
  WebGLRenderer,
  type Material,
} from 'three';
import { OrbitControls } from 'three/addons/controls/OrbitControls.js';
import type { Universe } from '../shared/types';

export interface UniverseHandle {
  setUniverse(universe: Universe): void;
  /** `callback` gets the `id` of the Planet someone clicked. */
  onSelect(callback: (planetId: number) => void): void;
  /** `callback` gets true when the browser takes the drawing context away, and false if it gives it back. */
  onContextChange(callback: (lost: boolean) => void): void;
  destroy(): void;
}

/** Thrown where this computer cannot start WebGL, so a screen can tell that apart from a bug. */
export class NoWebGL extends Error {}

const FIELD_OF_VIEW = 45;
const STARS = 1600;
/** A click that moves further than this, in pixels, was a drag. */
const CLICK_SLOP = 4;

/** Stars have to land in the same places each time, or the app and the Share Site would differ. */
function starPositions(radius: number): number[] {
  let seed = 20240;
  const next = () => (seed = (seed * 16807) % 2147483647) / 2147483647;
  const positions: number[] = [];
  for (let i = 0; i < STARS; i += 1) {
    const z = next() * 2 - 1;
    const angle = next() * Math.PI * 2;
    const around = Math.sqrt(1 - z * z);
    const distance = radius * (2.5 + next() * 2.5);
    positions.push(around * Math.cos(angle) * distance, z * distance, around * Math.sin(angle) * distance);
  }
  return positions;
}

function drawing(universe: Universe, middle: Vector3, radius: number) {
  const sky = new Group();

  const planets = new InstancedMesh(
    new SphereGeometry(1, 32, 24),
    new MeshStandardMaterial({ roughness: 0.72, metalness: 0.05 }),
    Math.max(universe.planets.length, 1),
  );
  planets.count = universe.planets.length;
  const matrix = new Matrix4();
  const color = new Color();
  const hues = new Map(universe.galaxies.map((g) => [g.id, g.hue]));
  universe.planets.forEach((planet, i) => {
    const [x, y, z] = planet.position;
    matrix.makeScale(planet.radius, planet.radius, planet.radius).setPosition(x, y, z);
    planets.setMatrixAt(i, matrix);
    planets.setColorAt(i, color.setHSL(hues.get(planet.galaxy) ?? 0, 0.62, 0.6));
  });
  sky.add(planets);

  const ends = universe.links.flatMap((link) => [
    ...universe.planets[link.a].position,
    ...universe.planets[link.b].position,
  ]);
  sky.add(
    new LineSegments(
      new BufferGeometry().setAttribute('position', new Float32BufferAttribute(ends, 3)),
      new LineBasicMaterial({ color: 0xece8da, transparent: true, opacity: 0.3 }),
    ),
  );

  const stars = new Points(
    new BufferGeometry().setAttribute('position', new Float32BufferAttribute(starPositions(radius), 3)),
    new PointsMaterial({ color: 0xc9cfe0, size: 1.3, sizeAttenuation: false, transparent: true, opacity: 0.7 }),
  );
  stars.position.copy(middle);
  sky.add(stars);

  return { sky, planets };
}

export function mountUniverse(container: HTMLElement, first: Universe): UniverseHandle {
  let renderer: WebGLRenderer;
  try {
    renderer = new WebGLRenderer({ antialias: true, alpha: true });
  } catch (e) {
    throw new NoWebGL(String(e));
  }
  renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
  renderer.domElement.style.display = 'block';
  container.append(renderer.domElement);

  const scene = new Scene();
  scene.add(new AmbientLight(0xb8c4e8, 0.7));
  const sun = new DirectionalLight(0xfff2dc, 2.6);
  sun.position.set(-1, 1.2, 1.4);
  scene.add(sun);

  const camera = new PerspectiveCamera(FIELD_OF_VIEW);
  const controls = new OrbitControls(camera, renderer.domElement);
  const render = () => renderer.render(scene, camera);
  // Only redraws when something changes, so a still Universe costs no battery.
  controls.addEventListener('change', render);

  let universe = first;
  let current: ReturnType<typeof drawing> | undefined;

  function fitToWindow() {
    const { clientWidth: width, clientHeight: height } = container;
    if (!width || !height) return;
    renderer.setSize(width, height);
    camera.aspect = width / height;
    camera.updateProjectionMatrix();
  }

  function setUniverse(next: Universe) {
    universe = next;
    release();
    const middle = new Vector3();
    next.planets.forEach((p) => middle.add(new Vector3(...p.position)));
    if (next.planets.length) middle.divideScalar(next.planets.length);
    const radius = Math.max(1, ...next.planets.map((p) => new Vector3(...p.position).distanceTo(middle) + p.radius));

    current = drawing(next, middle, radius);
    scene.add(current.sky);

    // Far enough back to see all of it, whichever way the window is shaped.
    fitToWindow();
    const distance = radius / Math.sin((FIELD_OF_VIEW * Math.PI) / 360) / Math.min(camera.aspect, 1);
    camera.near = distance / 100;
    camera.far = distance + radius * 8;
    camera.updateProjectionMatrix();
    controls.maxDistance = distance * 2.5;
    controls.target.copy(middle);
    camera.position.copy(middle).add(new Vector3(0, 0.25, 1).normalize().multiplyScalar(distance));
    controls.update();
    render();
  }

  function release() {
    if (!current) return;
    scene.remove(current.sky);
    current.sky.traverse((object) => {
      const { geometry, material } = object as unknown as { geometry?: BufferGeometry; material?: Material };
      geometry?.dispose();
      material?.dispose();
    });
    current.planets.dispose();
  }

  let select: ((planetId: number) => void) | undefined;
  const canvas = renderer.domElement;
  const raycaster = new Raycaster();
  let downAt: { x: number; y: number } | undefined;
  const onDown = (e: PointerEvent) => (downAt = { x: e.clientX, y: e.clientY });
  const onUp = (e: PointerEvent) => {
    const wasClick = downAt && Math.hypot(e.clientX - downAt.x, e.clientY - downAt.y) <= CLICK_SLOP;
    if (!wasClick || !current) return;
    const box = canvas.getBoundingClientRect();
    const pointer = new Vector2(
      ((e.clientX - box.left) / box.width) * 2 - 1,
      -((e.clientY - box.top) / box.height) * 2 + 1,
    );
    raycaster.setFromCamera(pointer, camera);
    const instance = raycaster.intersectObject(current.planets)[0]?.instanceId;
    if (instance !== undefined) select?.(universe.planets[instance].id);
  };
  canvas.addEventListener('pointerdown', onDown);
  canvas.addEventListener('pointerup', onUp);

  let contextChange: ((lost: boolean) => void) | undefined;
  const onContextLost = () => contextChange?.(true);
  // three.js restores its own state first, but this only draws when asked, so the sky stays empty until then.
  const onContextRestored = () => {
    render();
    contextChange?.(false);
  };
  canvas.addEventListener('webglcontextlost', onContextLost);
  canvas.addEventListener('webglcontextrestored', onContextRestored);

  const resize = new ResizeObserver(() => {
    fitToWindow();
    render();
  });
  resize.observe(container);

  setUniverse(first);

  return {
    setUniverse,
    onSelect: (callback) => (select = callback),
    onContextChange: (callback) => (contextChange = callback),
    destroy() {
      resize.disconnect();
      canvas.removeEventListener('pointerdown', onDown);
      canvas.removeEventListener('pointerup', onUp);
      canvas.removeEventListener('webglcontextlost', onContextLost);
      canvas.removeEventListener('webglcontextrestored', onContextRestored);
      controls.dispose();
      release();
      renderer.dispose();
      // A browser allows about 16 live contexts, and it only frees one when it gets round to it.
      renderer.forceContextLoss();
      canvas.remove();
    },
  };
}

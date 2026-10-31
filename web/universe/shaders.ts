// Everything in the sky is painted here. There are no image files and no post-processing pass, so the
// glow is just additive sprites and every surface comes from noise and each Planet's seed.
// Colours are written straight to the screen, with no tone mapping, so a value here is the value you see.

const NOISE = /* glsl */ `
  float hash(vec3 p) {
    p = fract(p * 0.3183099 + vec3(0.71, 0.113, 0.419));
    p *= 17.0;
    return fract(p.x * p.y * p.z * (p.x + p.y + p.z));
  }

  float noise(vec3 p) {
    vec3 i = floor(p);
    vec3 f = fract(p);
    f = f * f * (3.0 - 2.0 * f);
    return mix(
      mix(mix(hash(i), hash(i + vec3(1, 0, 0)), f.x), mix(hash(i + vec3(0, 1, 0)), hash(i + vec3(1, 1, 0)), f.x), f.y),
      mix(mix(hash(i + vec3(0, 0, 1)), hash(i + vec3(1, 0, 1)), f.x), mix(hash(i + vec3(0, 1, 1)), hash(i + vec3(1, 1, 1)), f.x), f.y),
      f.z
    );
  }

  float fbm(vec3 p) {
    float sum = 0.0;
    float amount = 0.5;
    for (int i = 0; i < 4; i++) {
      sum += amount * noise(p);
      p = p * 2.03 + 11.3;
      amount *= 0.5;
    }
    return sum / 0.9375;
  }
`;

// Keeps a slow gradient from showing steps in 8 bit colour.
const DITHER = /* glsl */ `
  float dither(float shift) {
    return (fract(52.9829189 * fract(dot(gl_FragCoord.xy + shift, vec2(0.06711056, 0.00583715)))) - 0.5) / 255.0;
  }
`;

export const planetVertex = /* glsl */ `
  attribute float seed;
  attribute float rock;
  varying vec3 vSurface;
  varying vec3 vNormal;
  varying vec3 vView;
  varying vec3 vTint;
  varying float vSeed;
  varying float vRock;
  varying mat3 vTurn;

  void main() {
    vSurface = position;
    vTint = instanceColor;
    vSeed = seed;
    vRock = rock;
    // From the Planet's own surface into view space, with its size taken out.
    vTurn = normalMatrix * mat3(instanceMatrix) / length(instanceMatrix[0].xyz);
    vNormal = vTurn * normal;
    vec4 view = modelViewMatrix * instanceMatrix * vec4(position, 1.0);
    vView = view.xyz;
    gl_Position = projectionMatrix * view;
  }
`;

export const planetFragment = /* glsl */ `
  uniform vec2 distant;
  varying vec3 vSurface;
  varying vec3 vNormal;
  varying vec3 vView;
  varying vec3 vTint;
  varying float vSeed;
  varying float vRock;
  varying mat3 vTurn;

  ${NOISE}
  ${DITHER}

  const vec3 IVORY = vec3(0.96, 0.92, 0.82);
  // The sun stays over the viewer's left shoulder, so no way of turning the sky leaves a Planet dark.
  const vec3 SUN = vec3(-0.5, 0.56, 0.66);

  float from(float n) {
    return fract(sin(vSeed * 91.17 + n * 12.9898) * 43758.5453);
  }

  // Latitude bands that a slow noise shears out of line, with a storm somewhere on most of them.
  vec3 gasGiant(vec3 p) {
    float count = mix(3.5, 7.5, from(1.0));
    float shear = fbm(vec3(p.x * 1.4, p.y * 3.2, p.z * 1.4) + vSeed * 37.0) - 0.5;
    float y = p.y + shear * mix(0.1, 0.26, from(2.0));
    float aliased = smoothstep(0.25, 1.2, fwidth(y) * count * 3.0);
    float broad = sin(y * count * 3.1416 + from(3.0) * 6.28);
    float fine = sin(y * count * 8.7 + from(4.0) * 6.28) * (1.0 - aliased);
    float streaks = fbm(vec3(p.x * 2.2, y * 24.0, p.z * 2.2) + vSeed * 13.0) - 0.5;
    float t = 0.5 + 0.3 * broad * (1.0 - aliased * 0.7) + 0.1 * fine + 0.5 * streaks * (1.0 - aliased);

    vec3 shade = vTint * mix(0.46, 0.62, from(5.0));
    vec3 pale = mix(vTint, IVORY, mix(0.28, 0.55, from(6.0)));
    vec3 color = mix(shade, vTint, smoothstep(0.1, 0.5, t));
    color = mix(color, pale, smoothstep(0.55, 0.95, t));

    vec3 at = normalize(vec3(cos(from(7.0) * 6.28), (from(8.0) - 0.5) * 1.1, sin(from(7.0) * 6.28)));
    vec3 off = p - at;
    float storm = (1.0 - smoothstep(0.12, 0.34, length(off * vec3(1.0, 2.6, 1.0)))) * step(0.3, from(9.0));
    float swirl = sin(length(off * vec3(1.0, 2.6, 1.0)) * 46.0 + shear * 18.0) * 0.5 + 0.5;
    return mix(color, mix(pale, shade, swirl * 0.5), storm * 0.85);
  }

  // Every crater near this point added up: how the ground slopes in xyz, and how high it stands in w.
  // The slope is worked out from the crater's shape, not read off neighbouring pixels, so it stays smooth
  // however close the camera comes. The shape is a bowl, plus a lip that has faded to nothing by 1.4.
  vec4 craters(vec3 p, float scale) {
    vec3 cell = floor(p * scale);
    vec3 inside = fract(p * scale);
    vec4 sum = vec4(0.0);
    for (int x = -1; x <= 1; x++) for (int y = -1; y <= 1; y++) for (int z = -1; z <= 1; z++) {
      vec3 near = cell + vec3(x, y, z);
      if (hash(near + 21.7) < 0.3) continue;
      vec3 centre = vec3(x, y, z) + vec3(hash(near), hash(near + 3.1), hash(near + 7.7));
      float size = 0.2 + 0.34 * hash(near + 13.3);
      vec3 away = inside - centre;
      float d = length(away) / size;
      if (d >= 1.4) continue;
      float inner = max(1.0 - d * d, 0.0);
      float off = (d - 1.0) / 0.17;
      float lip = 0.34 * exp(-off * off);
      float height = lip - inner * inner;
      float slope = 4.0 * d * inner - lip * 2.0 * off / 0.17;
      sum += vec4(away / max(length(away), 0.0001) * slope * size, height);
    }
    return sum;
  }

  // Bare stone in the Galaxy's colour, pitted at two scales. relief is how the ground slopes.
  vec3 rockSurface(vec3 p, out vec3 relief) {
    vec3 q = p + vSeed * 29.0;
    float ground = fbm(q * 2.3);
    float grain = fbm(q * 9.0);
    float fine = 1.0 - smoothstep(0.03, 0.14, fwidth(p.x) + fwidth(p.y));
    vec4 big = craters(q, 2.5);
    vec4 tiny = craters(q + 5.0, 6.0) * fine;
    relief = big.xyz * 0.62 + tiny.xyz * 0.26;

    vec3 stone = mix(vec3(0.3, 0.29, 0.28), vTint, 0.6);
    vec3 color = mix(stone * 0.58, stone * 1.28, smoothstep(0.25, 0.75, ground));
    color *= 0.88 + 0.32 * (grain - 0.5);
    color *= 1.0 + clamp(big.w, -1.0, 0.4) * 0.4 + clamp(tiny.w, -1.0, 0.4) * 0.16;
    return color;
  }

  void main() {
    vec3 n = normalize(vNormal);
    vec3 v = normalize(-vView);
    vec3 sun = normalize(SUN);
    vec3 p = normalize(vSurface);
    vec3 surface;
    vec3 shaded = n;

    if (vRock > 0.5) {
      vec3 relief;
      surface = rockSurface(p, relief);
      // Only the part of the slope along the surface bends the light.
      shaded = normalize(n - vTurn * (relief - p * dot(relief, p)));
    } else {
      surface = gasGiant(p);
    }

    float facing = dot(shaded, sun);
    float light = smoothstep(-0.22, 0.95, facing);
    vec3 color = surface * (0.1 + 1.12 * light);
    // The night side picks up a little of the sky.
    color += surface * vec3(0.03, 0.06, 0.1) * (1.0 - light);

    float edge = 1.0 - max(dot(n, v), 0.0);
    vec3 air = mix(vTint, vec3(0.62, 0.8, 0.95), 0.35);
    float thick = vRock > 0.5 ? 0.42 : 0.85;
    color += air * thick * pow(edge, 3.2) * (0.25 + 0.95 * smoothstep(-0.35, 0.7, dot(n, sun)));

    // Haze, so a far Planet sits behind a near one.
    color *= mix(1.0, 0.6, smoothstep(distant.x, distant.y, -vView.z));
    gl_FragColor = vec4(color + dither(0.0), 1.0);
  }
`;

export const ringVertex = /* glsl */ `
  varying float vAcross;
  varying vec3 vTint;
  varying float vDepth;

  void main() {
    vAcross = length(position.xy);
    vTint = instanceColor;
    vec4 view = modelViewMatrix * instanceMatrix * vec4(position, 1.0);
    vDepth = -view.z;
    gl_Position = projectionMatrix * view;
  }
`;

export const ringFragment = /* glsl */ `
  uniform vec2 distant;
  uniform float inner;
  uniform float outer;
  varying float vAcross;
  varying vec3 vTint;
  varying float vDepth;

  void main() {
    float t = (vAcross - inner) / (outer - inner);
    // A wide band and a fine one with a gap between them, fading at both edges. On a far-off Planet the
    // fine one is thinner than a pixel, where it would only show as dots, so it goes first.
    float w = fwidth(t);
    float wide = smoothstep(0.0, max(0.14, w), t) * (1.0 - smoothstep(0.66 - max(0.12, w), 0.66, t));
    float fine = smoothstep(0.78, 0.84, t) * (1.0 - smoothstep(0.94, 1.0, t)) * (1.0 - smoothstep(0.12, 0.3, w));
    vec3 color = mix(vTint, vec3(0.96, 0.92, 0.82), 0.5) * mix(1.0, 0.6, smoothstep(distant.x, distant.y, vDepth));
    gl_FragColor = vec4(color, max(wide * 0.62, fine * 0.8));
  }
`;

// A square that always faces the camera, shared by the glow round a Planet and the dust round a Galaxy.
const BILLBOARD = /* glsl */ `
  vec4 billboard(vec3 centre, vec2 corner, float size) {
    vec4 view = modelViewMatrix * vec4(centre, 1.0);
    view.xy += corner * size;
    return view;
  }
`;

export const glowVertex = /* glsl */ `
  varying vec2 vCorner;
  varying vec3 vTint;
  varying float vShift;
  ${BILLBOARD}

  void main() {
    vCorner = position.xy;
    vShift = fract(dot(instanceMatrix[3].xyz, vec3(12.9898, 78.233, 37.719))) * 400.0;
    float size = length(instanceMatrix[0].xyz);
    vec4 view = billboard(instanceMatrix[3].xyz, position.xy, size);
    // Inside a cloud there is nothing to see but fog, so it fades as the camera reaches it.
    vTint = instanceColor * smoothstep(0.2, 1.4, -view.z / size);
    gl_Position = projectionMatrix * view;
  }
`;

export const glowFragment = /* glsl */ `
  varying vec2 vCorner;
  varying vec3 vTint;
  varying float vShift;
  ${DITHER}

  void main() {
    float d = dot(vCorner, vCorner);
    if (d >= 1.0) discard;
    float fall = (exp(-d * 4.6) - 0.01005) / 0.98995;
    // Each sprite shifts its dither, or a pile of them would add the same pattern into a visible screen.
    gl_FragColor = vec4(vTint * fall + dither(vShift), 1.0);
  }
`;

// The brass scale round the chosen Planet, like the mark on the nav rail and the pointer on a dial.
export const reticleVertex = /* glsl */ `
  uniform vec3 centre;
  uniform float size;
  varying vec2 vCorner;
  ${BILLBOARD}

  void main() {
    vCorner = position.xy;
    gl_Position = projectionMatrix * billboard(centre, position.xy, size);
  }
`;

export const reticleFragment = /* glsl */ `
  varying vec2 vCorner;

  void main() {
    float r = length(vCorner);
    float w = fwidth(r);
    float line = 1.0 - smoothstep(0.0, w * 1.4, abs(r - 0.9));
    float turn = atan(vCorner.y, vCorner.x) / 6.28318530718 + 0.5;
    float ticks = turn * 72.0;
    float across = abs(fract(ticks + 0.5) - 0.5) / 72.0 * 6.28318530718 * r;
    float major = step(fract(turn * 12.0 + 0.0417), 0.0834);
    float reach = mix(0.935, 0.975, major);
    float tick = (1.0 - smoothstep(0.0, w * 1.2, across)) * step(0.9, r) * step(r, reach);
    float alpha = max(line * 0.9, tick * mix(0.55, 0.95, major));
    if (alpha <= 0.0) discard;
    gl_FragColor = vec4(0.84, 0.66, 0.33, alpha);
  }
`;

// Links and the instrument's rings. Each vertex carries its own colour and strength, and lines dim with
// distance, which is most of what makes a thin line read as being in space.
export const lineVertex = /* glsl */ `
  attribute vec4 tint;
  varying vec4 vTint;
  varying float vDepth;

  void main() {
    vTint = tint;
    vec4 view = modelViewMatrix * vec4(position, 1.0);
    vDepth = -view.z;
    gl_Position = projectionMatrix * view;
  }
`;

export const lineFragment = /* glsl */ `
  uniform vec2 distant;
  varying vec4 vTint;
  varying float vDepth;

  void main() {
    gl_FragColor = vec4(vTint.rgb, vTint.a * mix(1.0, 0.3, smoothstep(distant.x, distant.y, vDepth)));
  }
`;

// The sky itself: a dense sphere with its colour worked out at each corner, so it costs almost nothing
// per pixel. Deep sea-blue, a warm band like a river of far stars, and dark lanes through it. Never violet.
export const domeVertex = /* glsl */ `
  varying vec3 vColor;
  ${NOISE}

  void main() {
    vec3 d = normalize(position);
    vec3 band = normalize(vec3(0.28, 0.82, -0.5));
    float across = dot(d, band);
    float river = exp(-across * across * 9.0);
    float clouds = fbm(d * 2.1 + 4.0);
    float lanes = fbm(d * 3.8 + 17.0);
    vec3 color = vec3(0.038, 0.054, 0.084);
    color += vec3(0.008, 0.052, 0.072) * smoothstep(0.38, 0.7, clouds);
    color += vec3(0.13, 0.108, 0.078) * river * smoothstep(0.22, 0.72, clouds) * (1.0 - 0.85 * smoothstep(0.44, 0.62, lanes));
    color *= 1.0 - 0.3 * smoothstep(0.5, 0.7, lanes);
    vColor = color;
    // Infinitely far away, so it never moves as the camera flies and nothing can come in front of nothing.
    gl_Position = (projectionMatrix * vec4(mat3(viewMatrix) * d, 1.0)).xyww;
  }
`;

export const domeFragment = /* glsl */ `
  varying vec3 vColor;
  ${DITHER}

  void main() {
    gl_FragColor = vec4(vColor + dither(0.0), 1.0);
  }
`;

export const starVertex = /* glsl */ `
  attribute float size;
  attribute vec3 tint;
  uniform float pixels;
  varying vec3 vTint;

  void main() {
    vTint = tint;
    gl_PointSize = size * pixels;
    gl_Position = (projectionMatrix * vec4(mat3(viewMatrix) * position, 1.0)).xyww;
  }
`;

export const dustStarVertex = /* glsl */ `
  attribute float size;
  attribute vec3 tint;
  uniform float pixels;
  varying vec3 vTint;

  void main() {
    vTint = tint;
    gl_PointSize = size * pixels;
    gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0);
  }
`;

export const starFragment = /* glsl */ `
  varying vec3 vTint;

  void main() {
    float d = length(gl_PointCoord - 0.5) * 2.0;
    if (d >= 1.0) discard;
    gl_FragColor = vec4(vTint * (1.0 - smoothstep(0.1, 1.0, d)), 1.0);
  }
`;

// Visual projection of host-authoritative topology, for the live topology renderer.
// The SVG remains the accessible and authoritative interaction layer.

const VERTEX = `
  attribute vec2 a_position;
  varying vec2 v_uv;
  void main() {
    v_uv = a_position * 0.5 + 0.5;
    gl_Position = vec4(a_position, 0.0, 1.0);
  }
`;
const FRAGMENT = `
  precision highp float;
  varying vec2 v_uv;

  uniform vec2 u_resolution;
  uniform float u_offsetY;
  uniform float u_time;
  uniform vec2 u_selectedPos;
  uniform vec2 u_selectedSize;
  uniform float u_hasSelected;
  uniform float u_live;
  uniform vec2 u_pulseStart;
  uniform vec2 u_pulseEnd;
  uniform float u_pulseActive;
  uniform float u_scale;
  uniform vec4 u_segments[32];

  float sdSegment(vec2 p, vec2 a, vec2 b) {
    vec2 pa = p - a;
    vec2 ba = b - a;
    float h = clamp(dot(pa, ba) / max(1.0, dot(ba, ba)), 0.0, 1.0);
    return length(pa - ba * h);
  }

  float sdBoxOutline(vec2 p, vec2 c, vec2 b) {
    vec2 d = abs(p - c) - b;
    return abs(length(max(d, 0.0)) + min(max(d.x, d.y), 0.0));
  }

  float neonFalloff(float d, float tightScale, float wideScale) {
    float spec = 1.0 / (1.0 + d * d * tightScale);
    float atmo = exp(-d * wideScale);
    return spec * 0.58 + atmo * 0.42;
  }

  float hashNoise(vec2 p) {
    return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453123);
  }

  float valueNoise(vec2 p) {
    vec2 i = floor(p);
    vec2 f = fract(p);
    vec2 u = f * f * (3.0 - 2.0 * f);
    return mix(
      mix(hashNoise(i + vec2(0.0, 0.0)), hashNoise(i + vec2(1.0, 0.0)), u.x),
      mix(hashNoise(i + vec2(0.0, 1.0)), hashNoise(i + vec2(1.0, 1.0)), u.x),
      u.y
    );
  }

  float fogFbm(vec2 p) {
    float v = 0.0;
    float a = 0.5;
    mat2 rot = mat2(0.8, -0.6, 0.6, 0.8);
    for (int i = 0; i < 3; i++) {
      v += a * valueNoise(p);
      p = rot * p * 2.03 + vec2(17.3, 9.2);
      a *= 0.5;
    }
    return v / 0.875;
  }

  void main() {
    // The viewport-sized shader scrolls over the full-height authoritative SVG.
    vec2 fragPx = vec2(v_uv.x * u_resolution.x, (1.0 - v_uv.y) * u_resolution.y);
    float s = u_scale > 0.001 ? u_scale : 1.0;
    vec2 p = vec2(fragPx.x / s, (fragPx.y / s) + u_offsetY);

    // Deep canvas base with SDF light-field accumulation
    vec3 col = vec3(0.0, 0.0, 0.0);

    vec2 gMinor = abs(fract(p / 20.0) - 0.5);
    float lineMinor = max(
      smoothstep(0.476, 0.498, gMinor.x),
      smoothstep(0.476, 0.498, gMinor.y)
    );
    vec2 gMajor = abs(fract(p / 80.0) - 0.5);
    float lineMajor = max(
      smoothstep(0.489, 0.499, gMajor.x),
      smoothstep(0.489, 0.499, gMajor.y)
    );
    float crossHair = smoothstep(3.2, 0.0, length(gMajor * 80.0 - vec2(40.0)));

    // Slowly drifting atmospheric fog revealing the dim architectural background grid
    vec2 edgeDist = min(fragPx, u_resolution - fragPx);
    float edgeFade = smoothstep(0.0, 64.0, min(edgeDist.x, edgeDist.y));
    vec2 fogUv = p / 260.0;
    float fogA = fogFbm(fogUv + vec2(u_time * 0.05625, -u_time * 0.035));
    float fogB = fogFbm(fogUv * 1.45 + vec2(-u_time * 0.039, u_time * 0.045) + vec2(4.7, 8.3));
    float fogWave = smoothstep(0.40, 0.68, fogA * 0.58 + fogB * 0.42) * edgeFade;

    vec3 signalStart = vec3(0.769, 1.0, 0.0);
    vec3 signalEnd = vec3(0.0, 1.0, 0.769);
    float signalMix = clamp(
      v_uv.x * 0.55 + (1.0 - v_uv.y) * 0.45 + 0.08 * sin(u_time * 0.35 + (v_uv.x - v_uv.y) * 2.0),
      0.0,
      1.0
    );
    vec3 signalLight = mix(signalStart, signalEnd, signalMix);
    signalLight = mix(signalLight, vec3(0.96, 0.99, 0.98), 0.33);
    vec3 selectionLime = vec3(0.769, 1.0, 0.0);

    // Orthogonal Conduit Path SDF Backdrop (smooth continuous gradient)
    for (int i = 0; i < 32; i++) {
      vec4 seg = u_segments[i];
      vec2 ba = seg.zw - seg.xy;
      if (dot(ba, ba) > 1.0) {
        float dSeg = sdSegment(p, seg.xy, seg.zw);
        float segGlow = neonFalloff(dSeg, 0.026, 0.056) * (0.67 + 0.33 * fogWave);
        col += signalLight * segGlow * 0.22;
      }
    }

    // Selected entity dynamic fog extension (drifting lime atmosphere replacing the static box outline)
    float selAura = 0.0;
    float selProximity = 0.0;
    if (u_hasSelected > 0.5) {
      vec2 dBox = max(abs(p - u_selectedPos) - u_selectedSize * 0.5, 0.0);
      float dist = length(dBox);
      selAura = smoothstep(95.0, 0.0, dist);
      selProximity = exp(-dist * 0.042) * selAura;
      // Live drifting fog billows around the selected entity
      float selDrift = fogFbm(fogUv * 1.65 + vec2(u_time * 0.065, -u_time * 0.042) + vec2(1.8, 3.4));
      float dynamicFogGlow = selProximity * mix(fogWave, selDrift, 0.55);
      col += selectionLime * dynamicFogGlow * 0.55;
    }

    // Bounded Committed-Event Pulse Illumination (2.4s window)
    if (u_pulseActive > 0.5) {
      float dPulse = sdSegment(p, u_pulseStart, u_pulseEnd);
      vec2 ba = u_pulseEnd - u_pulseStart;
      float tProj = clamp(dot(p - u_pulseStart, ba) / max(1.0, dot(ba, ba)), 0.0, 1.0);
      float phase = fract(tProj - u_time * 1.25);
      float packet = exp(-pow((phase - 0.5) * 5.0, 2.0));
      col += signalLight * neonFalloff(dPulse, 0.014, 0.030) * (0.20 + 0.42 * packet);
    }

    // Background fog tone: ambient whitish-grey fog transitions to lime tint around the selected entity
    vec3 baseFogTone = vec3(0.50, 0.52, 0.56);
    vec3 fogTone = mix(baseFogTone, selectionLime, selAura * 0.85);
    float enhancedFogWave = fogWave + selAura * (0.35 + 0.65 * fogA) * 0.60;
    float bgPattern = lineMinor * 0.46 + lineMajor * 1.10 + crossHair * 1.00;
    vec3 fogCol = fogTone * enhancedFogWave * (0.09 + bgPattern * 0.32);

    // Reactive Architectural Grid with ~3% minimum baseline glow
    float signalLum = dot(col, vec3(0.299, 0.587, 0.114));
    col += fogCol;
    float gridPattern = lineMinor * 0.30 + lineMajor * 0.56;
    float gridIllum = clamp(fogWave * 0.72 + signalLum * 0.45, 0.0, 1.0);
    vec3 gridColor = vec3(0.030) + vec3(0.35) * gridIllum;
    col += gridColor * gridPattern;

    float lum = dot(col, vec3(0.299, 0.587, 0.114));
    float lightMask = clamp(signalLum * 2.0 + fogWave * 0.62 + gridPattern * 0.06, 0.0, 1.0);

    float scanline = 0.92 + 0.10 * sin(fragPx.y * 1.570796);
    float grain = (hashNoise(fragPx) - 0.5) * 0.022 * lightMask;
    col = max(vec3(0.0), col * scanline + grain);

    gl_FragColor = vec4(col, clamp(lum * 3.0 + fogWave * 0.62 + gridPattern * 0.08, 0.0, 0.90));
  }
`;
const contexts = new WeakMap();
const unavailable = new WeakSet();

/** @param {any} graph
 * @param {string} focusId */
export function relatedTopologyIds(graph, focusId = "") {
  const relatedIds = new Set();
  if (!focusId) return relatedIds;
  relatedIds.add(focusId);
  const focusedNode = graph.nodes.find(/** @param {any} node */ (node) => node.id === focusId);
  const walkDownstream = focusedNode?.kind === "host" || focusedNode?.kind === "provider";
  let changed = true;
  while (changed) {
    changed = false;
    for (const edge of graph.edges) {
      if (relatedIds.has(edge.to) && !relatedIds.has(edge.from)) {
        relatedIds.add(edge.from);
        changed = true;
      }
      if (walkDownstream && relatedIds.has(edge.from) && !relatedIds.has(edge.to)) {
        relatedIds.add(edge.to);
        changed = true;
      }
    }
  }
  return relatedIds;
}

/** @param {string} path */
export function edgeEndpoints(path) {
  const tokens = String(path || "").match(/[A-Z]|-?\d+(?:\.\d+)?/g) || [];
  let command = "";
  let x1 = 0;
  let y1 = 0;
  let x2 = 0;
  let y2 = 0;
  for (let index = 0; index < tokens.length;) {
    if (/^[A-Z]$/.test(tokens[index])) command = tokens[index++];
    if (command === "M") {
      x1 = x2 = Number(tokens[index++]);
      y1 = y2 = Number(tokens[index++]);
      command = "L";
    } else if (command === "H") {
      x2 = Number(tokens[index++]);
    } else if (command === "V") {
      y2 = Number(tokens[index++]);
    } else {
      index++;
    }
  }
  return { x1, y1, x2, y2 };
}

/**
 * @param {string} path
 * @param {number} [minSpan]
 */
export function edgeGradientCoords(path, minSpan = 200) {
  const { x1, y1, x2, y2 } = edgeEndpoints(path);
  const dx = x2 - x1;
  const dy = y2 - y1;
  const len = Math.hypot(dx, dy);
  if (len < 1) {
    return { x1, y1, x2: x1 + minSpan, y2 };
  }
  const span = Math.max(minSpan, len * 1.35);
  const mx = (x1 + x2) / 2;
  const my = (y1 + y2) / 2;
  const ux = dx / len;
  const uy = dy / len;
  return {
    x1: Math.round(mx - ux * (span / 2)),
    y1: Math.round(my - uy * (span / 2)),
    x2: Math.round(mx + ux * (span / 2)),
    y2: Math.round(my + uy * (span / 2)),
  };
}

/** @param {string} path */
function orthogonalSegments(path) {
  const tokens = path.match(/[A-Z]|-?\d+(?:\.\d+)?/g) || [];
  const segments = [];
  let command = "";
  let x = 0;
  let y = 0;
  for (let index = 0; index < tokens.length;) {
    if (/^[A-Z]$/.test(tokens[index])) command = tokens[index++];
    if (command === "M") {
      x = Number(tokens[index++]);
      y = Number(tokens[index++]);
      command = "L";
    } else if (command === "H") {
      const nextX = Number(tokens[index++]);
      if (nextX !== x) segments.push([x, y, nextX, y]);
      x = nextX;
    } else if (command === "V") {
      const nextY = Number(tokens[index++]);
      if (nextY !== y) segments.push([x, y, x, nextY]);
      y = nextY;
    } else {
      index++;
    }
  }
  return segments;
}

/** @param {any} canvas */
function create(canvas) {
  // The lightfield renders on state changes, not on an idle animation loop.
  // Preserve the framebuffer across compositor frames.
  const gl = canvas.getContext('webgl', {
    alpha:true, antialias:false, depth:false, stencil:false, preserveDrawingBuffer:true,
  });
  if (!gl) { unavailable.add(canvas); return null; }
  const compile = /** @param {any} kind
 * @param {any} source */
                  (kind, source) => {
    const shader = gl.createShader(kind);
    if (!shader) return null;
    gl.shaderSource(shader, source);
    gl.compileShader(shader);
    if (!gl.getShaderParameter(shader, gl.COMPILE_STATUS)) { gl.deleteShader(shader); return null; }
    return shader;
  };
  const vs = compile(gl.VERTEX_SHADER, VERTEX), fs = compile(gl.FRAGMENT_SHADER, FRAGMENT);
  if (!vs || !fs) {
    if (vs) gl.deleteShader(vs);
    if (fs) gl.deleteShader(fs);
    unavailable.add(canvas);
    return null;
  }
  const program = gl.createProgram();
  gl.attachShader(program, vs); gl.attachShader(program, fs); gl.linkProgram(program);
  if (!gl.getProgramParameter(program, gl.LINK_STATUS)) {
    gl.deleteShader(vs); gl.deleteShader(fs); gl.deleteProgram(program);
    unavailable.add(canvas);
    return null;
  }
  gl.useProgram(program);
  const buffer = gl.createBuffer();
  gl.bindBuffer(gl.ARRAY_BUFFER, buffer);
  gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([-1,-1, 1,-1, -1,1, -1,1, 1,-1, 1,1]), gl.STATIC_DRAW);
  const a = gl.getAttribLocation(program, 'a_position');
  gl.enableVertexAttribArray(a); gl.vertexAttribPointer(a, 2, gl.FLOAT, false, 0, 0);
  const uniforms = Object.fromEntries(['u_resolution','u_offsetY','u_time','u_selectedPos','u_selectedSize','u_hasSelected','u_live','u_pulseStart','u_pulseEnd','u_pulseActive','u_scale','u_segments[0]'].map(k=>[k,gl.getUniformLocation(program,k)]));
  /** @type {any} */
  const ctx = {gl,program,buffer,vs,fs,uniforms}; contexts.set(canvas,ctx);
  canvas.addEventListener('webglcontextlost', /** @param {any} e */
                                              e => {
    e.preventDefault();
    if (ctx.frame) cancelAnimationFrame(ctx.frame);
    contexts.delete(canvas);
    unavailable.add(canvas);
  });
  canvas.addEventListener('webglcontextrestored', () => unavailable.delete(canvas));
  return ctx;
}
/** @param {any} canvas */
export function releaseTopology(canvas) {
  if (canvas) unavailable.delete(canvas);
  const ctx = canvas && contexts.get(canvas);
  if (!ctx) return;
  const {gl,buffer,vs,fs,program}=ctx;
  if (ctx.frame) cancelAnimationFrame(ctx.frame);
  gl.deleteBuffer(buffer); gl.deleteShader(vs); gl.deleteShader(fs); gl.deleteProgram(program);
  contexts.delete(canvas);
}

// Bounded shader uniforms describe only graph geometry visible within the map.
// All other nodes and edges remain accessible in the full native SVG.
/** @param {any} graph
 * @param {any} selected
 * @param {any} scrollY
 * @param {any} height
 * @param {string} focusId */
export function lightfieldGeometry(graph, selected, scrollY, height, focusId = "") {
  const top = scrollY - 50;
  const bottom = scrollY + height + 50;
  const visibleNode = /** @param {any} node */ (node) => node.y + node.h >= top && node.y <= bottom;
  const priority = [
    ...graph.nodes.filter(/** @param {any} node */ (node) => node.kind === 'session' &&
      node.hostId === selected?.hostId && node.uri === selected?.uri && visibleNode(node)),
    ...graph.nodes.filter(/** @param {any} node */ (node) => node.kind === 'host' && visibleNode(node)),
    ...graph.nodes.filter(/** @param {any} node */ (node) => visibleNode(node)),
  ];
  const seen = new Set();
  const nodes = priority.filter((node) => !seen.has(node.id) && seen.add(node.id)).slice(0, 12);
  const selectedEdge = /** @param {any} edge */ (edge) => edge.hostId === selected?.hostId &&
    edge.sessionUri === selected?.uri;
  const relatedIds = relatedTopologyIds(graph, focusId);
  const focusedEdge = /** @param {any} edge */ (edge) =>
    relatedIds.has(edge.from) && relatedIds.has(edge.to);
  const segments = [];
  const seenSegments = new Set();
  const liveEdges = graph.edges.filter(/** @param {any} edge */ (edge) => !edge.stale);
  const orderedEdges = focusId
    ? liveEdges.filter(focusedEdge)
    : [...liveEdges.filter(selectedEdge), ...liveEdges.filter(/** @param {any} edge */ (edge) => !selectedEdge(edge))];
  for (const edge of orderedEdges) {
    for (const segment of orthogonalSegments(edge.d)) {
      if (segments.length === 32) return { nodes, segments };
      if (Math.max(segment[1], segment[3]) >= top && Math.min(segment[1], segment[3]) <= bottom &&
          (segment[0] !== segment[2] || segment[1] !== segment[3])) {
        const key = segment[0] < segment[2] || (segment[0] === segment[2] && segment[1] <= segment[3])
          ? `${segment[0]},${segment[1]},${segment[2]},${segment[3]}`
          : `${segment[2]},${segment[3]},${segment[0]},${segment[1]}`;
        if (!seenSegments.has(key)) {
          seenSegments.add(key);
          segments.push(segment);
        }
      }
    }
  }
  return { nodes, segments };
}

/** @param {any} canvas

 * @param {any} graph

 * @param {any} selected

 * @param {any} signal

 * @param {any} chatUri

 * @param {any} scrollY

 * @param {any} visibleHeight

 * @param {any} previewNodeId
 * @param {string} focusNodeId */
export function paintTopology(canvas, graph, selected, signal, chatUri, scrollY = 0, visibleHeight = graph?.height, previewNodeId = "", focusNodeId = "", zoom = 1) {
  if (!canvas || !graph) return;
  if (matchMedia('(prefers-reduced-motion: reduce)').matches) {
    const ctx = contexts.get(canvas);
    if (ctx?.frame) cancelAnimationFrame(ctx.frame);
    return;
  }
  if (unavailable.has(canvas)) return;
  const ctx = contexts.get(canvas) || create(canvas);
  if (!ctx) return;
  const {gl,program,uniforms:u}=ctx;
  if (ctx.frame) cancelAnimationFrame(ctx.frame);
  ctx.frame = 0;
  const offsetY = Math.max(0, Math.min(scrollY, graph.height - 1));
  const height = Math.max(1, Math.ceil(Math.min(visibleHeight || graph.height, graph.height - offsetY)));
  const dpr = Math.min(typeof window !== "undefined" ? window.devicePixelRatio || 1 : 1, 2);
  const maxDim = 2048;
  const rawW = Math.max(1, Math.round(graph.width * (zoom || 1) * dpr));
  const rawH = Math.max(1, Math.round(height * (zoom || 1) * dpr));
  const scaleLimit = Math.min(1, maxDim / Math.max(rawW, rawH));
  const targetW = Math.max(1, Math.round(rawW * scaleLimit));
  const targetH = Math.max(1, Math.round(rawH * scaleLimit));
  if (canvas.width !== targetW || canvas.height !== targetH) {
    canvas.width = targetW;
    canvas.height = targetH;
  }
  gl.viewport(0, 0, canvas.width, canvas.height);
  gl.useProgram(program);
  gl.enable(gl.BLEND);
  gl.blendFunc(gl.SRC_ALPHA, gl.ONE_MINUS_SRC_ALPHA);
  const segments = new Float32Array(128);
  const geometry = lightfieldGeometry(graph, selected, offsetY, height, focusNodeId);
  geometry.segments.forEach((segment, index) => segments.set(segment, index * 4));
  const chosen=(previewNodeId&&graph.nodes.find(/** @param {any} n */
                                                n=>n.id===previewNodeId))||
    graph.nodes.find(/** @param {any} n */
                     n=>n.kind==='session'&&n.hostId===selected?.hostId&&n.uri===selected?.uri);
  gl.uniform2f(u.u_resolution, canvas.width, canvas.height);
  gl.uniform1f(u.u_offsetY, offsetY);
  gl.uniform1f(u.u_time, 0);
  gl.uniform1f(u.u_scale, Math.max(0.01, targetW / Math.max(1, graph.width)));
  gl.uniform2f(u.u_selectedPos,chosen?.x+(chosen?.w||0)/2||0,chosen?.y+(chosen?.h||0)/2||0);
  gl.uniform2f(u.u_selectedSize, chosen?.w||0, chosen?.h||0);
  gl.uniform1f(u.u_hasSelected,chosen?1:0);
  const live = graph.nodes.some(/** @param {any} node */ (node) =>
    ['connected','working','idle','attention'].includes(node.status));
  gl.uniform1f(u.u_live, live ? 1 : 0);
  gl.uniform4fv(u['u_segments[0]'],segments);
  const active = signal && signal.uri === chatUri && Date.now()-signal.at < 1400 &&
    graph.edges.find(/** @param {any} edge */
                     edge=>edge.hostId===selected?.hostId&&edge.sessionUri===selected?.uri);
  const points = active?.d?.match(/-?\d+(?:\.\d+)?/g)?.map(Number);
  gl.uniform2f(u.u_pulseStart, points?.[0]||0, points?.[1]||0);
  gl.uniform2f(u.u_pulseEnd, points?.[3]||0, points?.[4]||0);
  gl.uniform1f(u.u_pulseActive, active?1:0);
  if (!ctx.startedAt) ctx.startedAt = performance.now();
  const draw = () => {
    gl.clearColor(0, 0, 0, 0);
    gl.clear(gl.COLOR_BUFFER_BIT);
    const elapsed = (performance.now() - ctx.startedAt) / 1000;
    gl.uniform1f(u.u_time, active ? Math.max(0, (Date.now() - signal.at) / 1000) : elapsed);
    gl.drawArrays(gl.TRIANGLES,0,6);
    if ((active || live) && canvas.isConnected)
      ctx.frame = requestAnimationFrame(draw);
    else ctx.frame = 0;
  };
  draw();
}

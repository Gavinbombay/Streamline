# Shader Craft technique index

Use this catalog for a concise description of each technique and its implementation scope. The routing table in [SKILL.md](../SKILL.md) links to the implementation guides.

## Technique Index

### Geometry & SDF
- **sdf-2d** - 2D signed distance functions for shapes, UI, anti-aliased rendering
- **sdf-3d** - 3D signed distance functions for real-time implicit surface modeling
- **csg-boolean-operations** - Constructive solid geometry: union, subtraction, intersection with smooth blending
- **domain-repetition** - Infinite space repetition, folding, and limited tiling
- **domain-warping** - Distort domains with noise for organic, flowing shapes
- **sdf-tricks** - SDF optimization, bounding volumes, binary search refinement, hollowing, layered edges, debug visualization

### Ray Casting & Lighting
- **ray-marching** - Sphere tracing with SDF for 3D scene rendering
- **analytic-ray-tracing** - Closed-form ray-primitive intersections (sphere, plane, box, torus)
- **path-tracing-gi** - Monte Carlo path tracing for photorealistic global illumination
- **lighting-model** - Phong, Blinn-Phong, PBR (Cook-Torrance), and toon shading
- **shadow-techniques** - Hard shadows, soft shadows (penumbra estimation), cascade shadows
- **ambient-occlusion** - SDF-based AO, screen-space AO approximation
- **normal-estimation** - Finite-difference normals, tetrahedron technique

### Simulation & Physics
- **fluid-simulation** - Navier-Stokes fluid solver with advection, diffusion, pressure projection
- **simulation-physics** - GPU-based physics: springs, cloth, N-body gravity, collision
- **particle-system** - Stateless and stateful particle systems (fire, rain, sparks, galaxies)
- **cellular-automata** - Game of Life, reaction-diffusion (Turing patterns), sand simulation

### Natural Phenomena
- **water-ocean** - Gerstner waves, FFT ocean, caustics, underwater fog
- **terrain-rendering** - Heightfield ray marching, FBM terrain, erosion
- **atmospheric-scattering** - Rayleigh/Mie scattering, god rays, SSS approximation
- **volumetric-rendering** - Volume ray marching for clouds, fog, fire, explosions

### Procedural Generation
- **procedural-noise** - Value noise, Perlin, Simplex, Worley, FBM, ridged noise
- **procedural-2d-pattern** - Brick, hexagon, truchet, Islamic geometric patterns
- **voronoi-cellular-noise** - Voronoi diagrams, Worley noise, cracked earth, crystal
- **fractal-rendering** - Mandelbrot, Julia sets, 3D fractals (Mandelbox, Mandelbulb)
- **color-palette** - Cosine palettes, HSL/HSV/Oklab, dynamic color mapping

### Post-Processing & Infrastructure
- **post-processing** - Bloom, tone mapping (ACES, Reinhard), vignette, chromatic aberration, glitch
- **multipass-buffer** - Ping-pong FBO setup, state persistence across frames
- **texture-sampling** - Bilinear, bicubic, mipmap, procedural texture lookup
- **matrix-transform** - Camera look-at, projection, rotation, orbit controls
- **polar-uv-manipulation** - Polar/log-polar coordinates, kaleidoscope, spiral mapping
- **anti-aliasing** - SSAA, SDF analytical AA, temporal anti-aliasing (TAA), FXAA post-process
- **camera-effects** - Depth of field (thin lens), motion blur, lens distortion, film grain, vignette
- **texture-mapping-advanced** - Biplanar mapping, texture repetition avoidance, ray differential filtering

### Audio
- **sound-synthesis** - Procedural audio in GLSL: oscillators, envelopes, filters, FM synthesis

### Debugging & Validation
- **webgl-pitfalls** - Common WebGL2/GLSL errors: `fragCoord`, `main()` wrapper, function order, macro limitations, uniform null

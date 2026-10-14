/**
 * WebGL2 live preview renderer approximating Lightroom develop settings.
 * Single fragment shader, one draw per render. Not a Lightroom match; visually plausible.
 */
import type { CubeLut } from './cube';

export const HSL_BANDS = ['Red', 'Orange', 'Yellow', 'Green', 'Aqua', 'Blue', 'Purple', 'Magenta'] as const;
export type HslBand = (typeof HSL_BANDS)[number];

type N = number | undefined;

export type DevelopParams = {
	Exposure2012?: number;
	Contrast2012?: number;
	Highlights2012?: number;
	Shadows2012?: number;
	Whites2012?: number;
	Blacks2012?: number;
	Texture?: number;
	Clarity2012?: number;
	Dehaze?: number;
	Vibrance?: number;
	Saturation?: number;
	/** Mired delta vs as-shot, positive = warmer. */
	TemperatureShift?: number;
	/** -150..150, positive = magenta. */
	TintShift?: number;
	SplitToningShadowHue?: number;
	SplitToningShadowSaturation?: number;
	SplitToningHighlightHue?: number;
	SplitToningHighlightSaturation?: number;
	SplitToningBalance?: number;
	ColorGradeShadowLum?: number;
	ColorGradeMidtoneHue?: number;
	ColorGradeMidtoneSat?: number;
	ColorGradeMidtoneLum?: number;
	ColorGradeHighlightLum?: number;
	ColorGradeGlobalHue?: number;
	ColorGradeGlobalSat?: number;
	ColorGradeGlobalLum?: number;
	ColorGradeBlending?: number;
	ParametricShadows?: number;
	ParametricDarks?: number;
	ParametricLights?: number;
	ParametricHighlights?: number;
	ParametricShadowSplit?: number;
	ParametricMidtoneSplit?: number;
	ParametricHighlightSplit?: number;
	/** Flat [x0,y0,x1,y1,...] on 0..255. */
	ToneCurvePV2012?: number[];
	ToneCurvePV2012Red?: number[];
	ToneCurvePV2012Green?: number[];
	ToneCurvePV2012Blue?: number[];
	GrainAmount?: number;
	GrainSize?: number;
	PostCropVignetteAmount?: number;
	PostCropVignetteMidpoint?: number;
	ConvertToGrayscale?: boolean;
} & { [K in HslBand as `HueAdjustment${K}`]?: number } & {
	[K in HslBand as `SaturationAdjustment${K}`]?: number;
} & { [K in HslBand as `LuminanceAdjustment${K}`]?: number };

export interface RenderOptions {
	lutAmount?: number;
	/** 0..1 x position of before/after divider. Left side shows the original. undefined = off. */
	split?: number;
}

export interface Renderer {
	setImage(src: ImageBitmap | HTMLImageElement | HTMLCanvasElement): void;
	setLut(lut: CubeLut | null): void;
	render(params: DevelopParams, opts?: RenderOptions): void;
	destroy(): void;
}

// ---------------------------------------------------------------------------
// Tone curve baking (CPU)
// ---------------------------------------------------------------------------

/** Monotone cubic (Fritsch-Carlson) evaluator for points on 0..255. Returns f: 0..1 -> 0..1. */
function monotoneCurve(flat: number[] | undefined): ((x: number) => number) | null {
	if (!flat || flat.length < 4) return null;
	const pts: [number, number][] = [];
	for (let i = 0; i + 1 < flat.length; i += 2) pts.push([flat[i] / 255, flat[i + 1] / 255]);
	pts.sort((a, b) => a[0] - b[0]);
	const p = pts.filter((q, i) => i === 0 || q[0] > pts[i - 1][0] + 1e-6);
	const n = p.length;
	if (n < 2) return null;
	const h: number[] = [];
	const d: number[] = [];
	for (let i = 0; i < n - 1; i++) {
		h.push(p[i + 1][0] - p[i][0]);
		d.push((p[i + 1][1] - p[i][1]) / h[i]);
	}
	const m = new Array<number>(n);
	m[0] = d[0];
	m[n - 1] = d[n - 2];
	for (let i = 1; i < n - 1; i++) m[i] = d[i - 1] * d[i] <= 0 ? 0 : (d[i - 1] + d[i]) / 2;
	for (let i = 0; i < n - 1; i++) {
		if (d[i] === 0) {
			m[i] = 0;
			m[i + 1] = 0;
			continue;
		}
		const a = m[i] / d[i];
		const b = m[i + 1] / d[i];
		const s = a * a + b * b;
		if (s > 9) {
			const t = 3 / Math.sqrt(s);
			m[i] = t * a * d[i];
			m[i + 1] = t * b * d[i];
		}
	}
	return (x: number) => {
		if (x <= p[0][0]) return p[0][1];
		if (x >= p[n - 1][0]) return p[n - 1][1];
		let k = 0;
		while (k < n - 2 && x > p[k + 1][0]) k++;
		const t = (x - p[k][0]) / h[k];
		const t2 = t * t;
		const t3 = t2 * t;
		return (
			(2 * t3 - 3 * t2 + 1) * p[k][1] +
			(t3 - 2 * t2 + t) * h[k] * m[k] +
			(-2 * t3 + 3 * t2) * p[k + 1][1] +
			(t3 - t2) * h[k] * m[k + 1]
		);
	};
}

const sstep = (a: number, b: number, x: number) => {
	const t = Math.min(1, Math.max(0, (x - a) / (b - a)));
	return t * t * (3 - 2 * t);
};

/** Parametric region offset at x (0..1). Hat weights through region centres. */
function parametricOffset(x: number, v: number[], s: number[]): number {
	const s1 = s[0] / 100, s2 = s[1] / 100, s3 = s[2] / 100;
	const c = [s1 / 2, (s1 + s2) / 2, (s2 + s3) / 2, (s3 + 1) / 2];
	let off = 0;
	if (x <= c[0]) off = v[0];
	else if (x >= c[3]) off = v[3];
	else {
		let k = 0;
		while (k < 2 && x > c[k + 1]) k++;
		const t = sstep(c[k], c[k + 1], x);
		off = v[k] * (1 - t) + v[k + 1] * t;
	}
	return (off / 100) * 0.2;
}

function bakeCurves(p: DevelopParams, out: Uint8Array): boolean {
	const par = [p.ParametricShadows ?? 0, p.ParametricDarks ?? 0, p.ParametricLights ?? 0, p.ParametricHighlights ?? 0];
	const spl = [p.ParametricShadowSplit ?? 25, p.ParametricMidtoneSplit ?? 50, p.ParametricHighlightSplit ?? 75];
	const hasPar = par.some((v) => v !== 0);
	const master = monotoneCurve(p.ToneCurvePV2012);
	const ch = [monotoneCurve(p.ToneCurvePV2012Red), monotoneCurve(p.ToneCurvePV2012Green), monotoneCurve(p.ToneCurvePV2012Blue)];
	if (!hasPar && !master && !ch[0] && !ch[1] && !ch[2]) return false;
	for (let i = 0; i < 256; i++) {
		let x = i / 255;
		if (hasPar) x = Math.min(1, Math.max(0, x + parametricOffset(x, par, spl)));
		if (master) x = Math.min(1, Math.max(0, master(x)));
		for (let c = 0; c < 3; c++) {
			const f = ch[c];
			const y = f ? Math.min(1, Math.max(0, f(x))) : x;
			out[i * 4 + c] = Math.round(y * 255);
		}
		out[i * 4 + 3] = 255;
	}
	return true;
}

// ---------------------------------------------------------------------------
// Half float conversion for the LUT texture (RGB16F is filterable everywhere)
// ---------------------------------------------------------------------------

function toHalf(data: Float32Array): Uint16Array {
	const f32 = new Float32Array(1);
	const u32 = new Uint32Array(f32.buffer);
	const out = new Uint16Array(data.length);
	for (let i = 0; i < data.length; i++) {
		f32[0] = data[i];
		const x = u32[0];
		const sign = (x >>> 16) & 0x8000;
		let e = ((x >>> 23) & 0xff) - 127 + 15;
		let m = x & 0x7fffff;
		if (e <= 0) {
			if (e < -10) out[i] = sign;
			else {
				m = (m | 0x800000) >> (1 - e);
				out[i] = sign | ((m + 0x1000) >> 13);
			}
		} else if (e >= 31) out[i] = sign | 0x7bff;
		else out[i] = sign | (e << 10) | ((m + 0x1000) >> 13);
	}
	return out;
}

// ---------------------------------------------------------------------------
// Shaders
// ---------------------------------------------------------------------------

const VERT = `#version 300 es
in vec2 aPos;
out vec2 vUv;
void main() {
	vUv = vec2(aPos.x * 0.5 + 0.5, 0.5 - aPos.y * 0.5);
	gl_Position = vec4(aPos, 0.0, 1.0);
}`;

const FRAG = `#version 300 es
precision highp float;
precision highp sampler3D;
in vec2 vUv;
out vec4 outColor;

uniform sampler2D uImg;
uniform sampler2D uCurve;
uniform sampler3D uLut;
uniform vec2 uImgSize;
uniform vec2 uRes;

uniform vec3 uWB;
uniform float uExposure;
uniform float uContrast, uHighlights, uShadows, uWhites, uBlacks;
uniform float uTexture, uClarity, uDehaze;
uniform float uLodTex, uLodClar;
uniform float uVibrance, uSaturation;
uniform bool uUseCurve, uUseHSL, uUseGrade, uUseLut, uGray;
uniform float uHue[8], uSat[8], uLum[8];
uniform vec3 uGShadow, uGMid, uGHigh, uGGlobal; // hue deg, sat 0..1, lum -1..1
uniform float uBalance, uBlend;
uniform vec3 uLutDomMin, uLutDomScale;
uniform float uLutScale, uLutOffset, uLutAmount;
uniform float uGrain, uGrainSize;
uniform float uVigAmt, uVigMid;
uniform float uSplit; // <0 off

const vec3 W = vec3(0.2126, 0.7152, 0.0722);
const float CEN[8] = float[8](0.0, 30.0, 60.0, 120.0, 180.0, 240.0, 275.0, 315.0);

vec3 toLinear(vec3 c) {
	return mix(c / 12.92, pow((c + 0.055) / 1.055, vec3(2.4)), step(vec3(0.04045), c));
}
vec3 toSrgb(vec3 c) {
	c = max(c, 0.0);
	return mix(c * 12.92, 1.055 * pow(c, vec3(1.0 / 2.4)) - 0.055, step(vec3(0.0031308), c));
}
float encL(float y) { return toSrgb(vec3(y)).x; }
float decL(float p) { return toLinear(vec3(max(p, 0.0))).x; }

vec3 rgb2hsv(vec3 c) {
	vec4 K = vec4(0.0, -1.0 / 3.0, 2.0 / 3.0, -1.0);
	vec4 p = mix(vec4(c.bg, K.wz), vec4(c.gb, K.xy), step(c.b, c.g));
	vec4 q = mix(vec4(p.xyw, c.r), vec4(c.r, p.yzx), step(p.x, c.r));
	float d = q.x - min(q.w, q.y);
	float e = 1e-10;
	return vec3(abs(q.z + (q.w - q.y) / (6.0 * d + e)), d / (q.x + e), q.x);
}
vec3 hsv2rgb(vec3 c) {
	vec4 K = vec4(1.0, 2.0 / 3.0, 1.0 / 3.0, 3.0);
	vec3 p = abs(fract(c.xxx + K.xyz) * 6.0 - K.www);
	return c.z * mix(K.xxx, clamp(p - K.xxx, 0.0, 1.0), c.y);
}

float hash(vec2 p) {
	vec3 p3 = fract(vec3(p.xyx) * 0.1031);
	p3 += dot(p3, p3.yzx + 33.33);
	return fract((p3.x + p3.y) * p3.z);
}
float vnoise(vec2 p) {
	vec2 i = floor(p), f = fract(p);
	f = f * f * (3.0 - 2.0 * f);
	return mix(mix(hash(i), hash(i + vec2(1, 0)), f.x), mix(hash(i + vec2(0, 1)), hash(i + vec2(1, 1)), f.x), f.y);
}

// Blurred encoded luma from the mip chain (cheap local-contrast base).
float blurP(vec2 uv, float lod) {
	vec2 o = exp2(lod) * 0.75 / uImgSize;
	float s = 0.0;
	s += dot(textureLod(uImg, uv + vec2(o.x, o.y), lod).rgb, W);
	s += dot(textureLod(uImg, uv + vec2(-o.x, o.y), lod).rgb, W);
	s += dot(textureLod(uImg, uv + vec2(o.x, -o.y), lod).rgb, W);
	s += dot(textureLod(uImg, uv + vec2(-o.x, -o.y), lod).rgb, W);
	return s * 0.25; // already in encoded space (luma of encoded rgb)
}

float smoothS(float p) { float c = clamp(p, 0.0, 1.0); return c * c * (3.0 - 2.0 * c) + (p - c); }

// Tone in perceptual (sRGB encoded) space. p0 = original encoded luma for detail terms.
float tonePerceptual(float p, float detailTex, float detailClar) {
	// Black/white end points.
	float bp = -uBlacks * 0.08;
	float wp = 1.0 - uWhites * 0.15;
	p = (p - bp) / (wp - bp);
	float cp = clamp(p, 0.0, 1.0);
	// Shadows / highlights: luminance-masked, smooth.
	float ms = 1.0 - smoothstep(0.05, 0.7, cp);
	float mh = smoothstep(0.3, 0.95, cp);
	p += uShadows * 0.25 * ms * (0.4 + 0.6 * (1.0 - cp));
	p += uHighlights * 0.25 * mh;
	// Contrast: S-curve about mid grey.
	if (uContrast >= 0.0) p = mix(p, smoothS(p), uContrast);
	else p = mix(p, 0.5 + (p - 0.5) * 0.5, -uContrast);
	// Dehaze: remove veil and deepen shadows (or add veil).
	if (uDehaze > 0.0) p = (p - uDehaze * 0.18) / (1.0 - uDehaze * 0.18);
	else if (uDehaze < 0.0) p = mix(p, 0.78, -uDehaze * 0.3);
	// Local contrast.
	float mid = 4.0 * clamp(p, 0.0, 1.0) * (1.0 - clamp(p, 0.0, 1.0));
	p += detailTex * uTexture * 0.9;
	p += detailClar * uClarity * 0.8 * mid;
	return p;
}

void main() {
	vec2 uv = vUv;
	vec3 src = texture(uImg, uv).rgb;
	vec3 orig = src;
	vec3 c = toLinear(src);

	// White balance (linear gain) and exposure.
	c *= uWB * uExposure;

	// Tone: operate on luminance in perceptual space, scale RGB by ratio.
	{
		float Y = max(dot(c, W), 0.0);
		float p0 = encL(Y);
		float dTex = 0.0, dClar = 0.0;
		if (uTexture != 0.0 || uClarity != 0.0) {
			float po = encL(dot(toLinear(src), W)); // unedited luma
			if (uTexture != 0.0) dTex = po - blurP(uv, uLodTex);
			if (uClarity != 0.0) dClar = po - blurP(uv, uLodClar);
		}
		float Yt = decL(tonePerceptual(p0, dTex, dClar));
		float ratio = Yt / max(Y, 1e-3);
		c = c * min(ratio, 32.0);
		c += max(Yt - dot(c, W), 0.0);
	}

	vec3 e = clamp(toSrgb(c), 0.0, 1.0);

	// Tone curves (parametric + point + per-channel baked on CPU).
	if (uUseCurve) {
		e = vec3(
			texture(uCurve, vec2(e.r * (255.0 / 256.0) + 0.5 / 256.0, 0.5)).r,
			texture(uCurve, vec2(e.g * (255.0 / 256.0) + 0.5 / 256.0, 0.5)).g,
			texture(uCurve, vec2(e.b * (255.0 / 256.0) + 0.5 / 256.0, 0.5)).b);
	}

	// HSL by hue band with smooth interpolation between band centres.
	if (uUseHSL) {
		vec3 hsv = rgb2hsv(e);
		float h = hsv.x * 360.0;
		float hs = 0.0, ss = 0.0, ls = 0.0;
		for (int i = 0; i < 8; i++) {
			float a = CEN[i];
			int j = (i + 1) % 8;
			float b = i == 7 ? 360.0 : CEN[j];
			if (h >= a && h < b) {
				float t = smoothstep(0.0, 1.0, (h - a) / (b - a));
				hs = mix(uHue[i], uHue[j], t);
				ss = mix(uSat[i], uSat[j], t);
				ls = mix(uLum[i], uLum[j], t);
			}
		}
		float w = smoothstep(0.0, 0.12, hsv.y) * smoothstep(0.0, 0.05, hsv.z);
		hsv.x = fract(hsv.x + hs * (30.0 / 360.0) * w + 1.0);
		hsv.y = clamp(hsv.y * (1.0 + ss * w), 0.0, 1.0);
		hsv.z = clamp(hsv.z * exp2(ls * 0.9 * w), 0.0, 1.0);
		e = hsv2rgb(hsv);
	}

	// Grayscale conversion before colour grading so grading can still tint it.
	if (uGray) e = vec3(dot(e, W));

	// Colour grading: shadows / midtones / highlights / global with balance + blending.
	if (uUseGrade) {
		float L = dot(e, W);
		float width = mix(0.2, 0.5, uBlend);
		float pivot = 0.5 - uBalance * 0.25;
		float wh = smoothstep(pivot, pivot + width, L);
		float ws = 1.0 - smoothstep(pivot - width, pivot, L);
		float wm = max(1.0 - ws - wh, 0.0);
		vec3 acc = vec3(0.0);
		float lumMul = 1.0;
		vec3 g[4] = vec3[4](uGShadow, uGMid, uGHigh, uGGlobal);
		float wt[4] = float[4](ws, wm, wh, 1.0);
		for (int i = 0; i < 4; i++) {
			if (g[i].y > 0.0) {
				vec3 tint = hsv2rgb(vec3(g[i].x / 360.0, 1.0, 1.0));
				acc += (tint - dot(tint, W)) * g[i].y * wt[i] * 0.45;
			}
			lumMul *= 1.0 + g[i].z * wt[i] * 0.5;
		}
		e = clamp((e + acc) * lumMul, 0.0, 1.0);
	}

	// Vibrance (protects saturated colours and skin) and saturation.
	if (uVibrance != 0.0 || uSaturation != 0.0) {
		float L = dot(e, W);
		vec3 hsv = rgb2hsv(e);
		float hdeg = hsv.x * 360.0;
		float skin = 1.0 - 0.55 * smoothstep(40.0, 5.0, abs(hdeg - 25.0)) * smoothstep(0.1, 0.5, hsv.y);
		float k = 1.0 + uSaturation + uVibrance * (1.0 - hsv.y) * skin * (uVibrance > 0.0 ? 1.0 : 1.0);
		if (uDehaze > 0.0) k += uDehaze * 0.25;
		e = clamp(mix(vec3(L), e, max(k, 0.0)), 0.0, 1.0);
	} else if (uDehaze > 0.0) {
		float L = dot(e, W);
		e = clamp(mix(vec3(L), e, 1.0 + uDehaze * 0.25), 0.0, 1.0);
	}

	// 3D LUT, trilinear on the GPU.
	if (uUseLut && uLutAmount > 0.0) {
		vec3 q = clamp((e - uLutDomMin) * uLutDomScale, 0.0, 1.0) * uLutScale + uLutOffset;
		vec3 l = texture(uLut, q).rgb;
		e = mix(e, clamp(l, 0.0, 1.0), uLutAmount);
	}

	// Grain (static, in image space, strongest in midtones).
	if (uGrain > 0.0) {
		float gs = max(uGrainSize, 0.5);
		vec2 gp = uv * uImgSize / gs;
		float n = vnoise(gp) * 0.6 + vnoise(gp * 2.1 + 17.0) * 0.4 - 0.5;
		float L = dot(e, W);
		float m = 0.35 + 0.65 * 4.0 * L * (1.0 - L);
		e = clamp(e + n * uGrain * 0.5 * m, 0.0, 1.0);
	}

	// Post-crop vignette. Negative darkens, positive lightens.
	if (uVigAmt != 0.0) {
		float d = length((uv - 0.5) * 2.0);
		float inner = 0.25 + 0.9 * uVigMid;
		float v = smoothstep(inner, 1.55, d);
		if (uVigAmt < 0.0) e *= 1.0 + uVigAmt * v * 0.95;
		else e = mix(e, vec3(1.0), uVigAmt * v * 0.75);
	}

	// Before / after.
	if (uSplit >= 0.0) {
		float px = uRes.x;
		if (abs(uv.x * px - uSplit * px) < 1.0) { outColor = vec4(1.0); return; }
		if (uv.x < uSplit) e = orig;
	}
	outColor = vec4(clamp(e, 0.0, 1.0), 1.0);
}`;

// ---------------------------------------------------------------------------

export function createRenderer(canvas: HTMLCanvasElement): Renderer {
	const gl = canvas.getContext('webgl2', {
		alpha: false,
		antialias: false,
		preserveDrawingBuffer: true,
		premultipliedAlpha: false
	});
	if (!gl) throw new Error('WebGL2 is not available');

	const compile = (type: number, src: string) => {
		const s = gl.createShader(type)!;
		gl.shaderSource(s, src);
		gl.compileShader(s);
		if (!gl.getShaderParameter(s, gl.COMPILE_STATUS)) {
			const log = gl.getShaderInfoLog(s);
			gl.deleteShader(s);
			throw new Error('Shader compile failed: ' + log);
		}
		return s;
	};
	const vs = compile(gl.VERTEX_SHADER, VERT);
	const fs = compile(gl.FRAGMENT_SHADER, FRAG);
	const prog = gl.createProgram()!;
	gl.attachShader(prog, vs);
	gl.attachShader(prog, fs);
	gl.linkProgram(prog);
	if (!gl.getProgramParameter(prog, gl.LINK_STATUS)) {
		throw new Error('Program link failed: ' + gl.getProgramInfoLog(prog));
	}
	gl.useProgram(prog);

	const locs = new Map<string, WebGLUniformLocation | null>();
	const loc = (n: string) => {
		let l = locs.get(n);
		if (l === undefined) {
			l = gl.getUniformLocation(prog, n);
			locs.set(n, l);
		}
		return l;
	};
	const f1 = (n: string, v: number) => gl.uniform1f(loc(n), v);
	const i1 = (n: string, v: number) => gl.uniform1i(loc(n), v);
	const b1 = (n: string, v: boolean) => gl.uniform1i(loc(n), v ? 1 : 0);
	const f3 = (n: string, a: number, b: number, c: number) => gl.uniform3f(loc(n), a, b, c);

	// Fullscreen triangle.
	const vao = gl.createVertexArray();
	gl.bindVertexArray(vao);
	const vbo = gl.createBuffer();
	gl.bindBuffer(gl.ARRAY_BUFFER, vbo);
	gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([-1, -1, 3, -1, -1, 3]), gl.STATIC_DRAW);
	const aPos = gl.getAttribLocation(prog, 'aPos');
	gl.enableVertexAttribArray(aPos);
	gl.vertexAttribPointer(aPos, 2, gl.FLOAT, false, 0, 0);

	// Textures: 0 image, 1 curve, 2 LUT.
	const imgTex = gl.createTexture()!;
	const curveTex = gl.createTexture()!;
	const lutTex = gl.createTexture()!;
	i1('uImg', 0);
	i1('uCurve', 1);
	i1('uLut', 2);

	gl.activeTexture(gl.TEXTURE0);
	gl.bindTexture(gl.TEXTURE_2D, imgTex);
	gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA8, 1, 1, 0, gl.RGBA, gl.UNSIGNED_BYTE, new Uint8Array([0, 0, 0, 255]));

	gl.activeTexture(gl.TEXTURE1);
	gl.bindTexture(gl.TEXTURE_2D, curveTex);
	gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA8, 256, 1, 0, gl.RGBA, gl.UNSIGNED_BYTE, new Uint8Array(256 * 4));
	gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
	gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
	gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
	gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);

	const initLut = (size: number, data: Uint16Array) => {
		gl.activeTexture(gl.TEXTURE2);
		gl.bindTexture(gl.TEXTURE_3D, lutTex);
		gl.pixelStorei(gl.UNPACK_ALIGNMENT, 1);
		gl.texImage3D(gl.TEXTURE_3D, 0, gl.RGB16F, size, size, size, 0, gl.RGB, gl.HALF_FLOAT, data);
		gl.pixelStorei(gl.UNPACK_ALIGNMENT, 4);
		for (const p of [gl.TEXTURE_WRAP_S, gl.TEXTURE_WRAP_T, gl.TEXTURE_WRAP_R]) gl.texParameteri(gl.TEXTURE_3D, p, gl.CLAMP_TO_EDGE);
		gl.texParameteri(gl.TEXTURE_3D, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
		gl.texParameteri(gl.TEXTURE_3D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
	};
	initLut(2, new Uint16Array(2 * 2 * 2 * 3));

	let imgW = 1;
	let imgH = 1;
	let lut: CubeLut | null = null;
	const curveBuf = new Uint8Array(256 * 4);
	let curveKey = '';
	let curveActive = false;
	let destroyed = false;

	const num = (v: N, d = 0) => (v === undefined || !Number.isFinite(v) ? d : v);

	return {
		setImage(src) {
			const w = (src as HTMLImageElement).naturalWidth || (src as HTMLCanvasElement).width;
			const h = (src as HTMLImageElement).naturalHeight || (src as HTMLCanvasElement).height;
			imgW = w;
			imgH = h;
			gl.activeTexture(gl.TEXTURE0);
			gl.bindTexture(gl.TEXTURE_2D, imgTex);
			gl.pixelStorei(gl.UNPACK_FLIP_Y_WEBGL, false);
			gl.pixelStorei(gl.UNPACK_PREMULTIPLY_ALPHA_WEBGL, false);
			gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA8, gl.RGBA, gl.UNSIGNED_BYTE, src);
			gl.generateMipmap(gl.TEXTURE_2D);
			gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR_MIPMAP_LINEAR);
			gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
			gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
			gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
		},

		setLut(next) {
			lut = next;
			if (next) initLut(next.size, toHalf(next.data));
		},

		render(p, opts = {}) {
			if (destroyed) return;
			gl.useProgram(prog);
			gl.viewport(0, 0, canvas.width, canvas.height);

			gl.activeTexture(gl.TEXTURE0);
			gl.bindTexture(gl.TEXTURE_2D, imgTex);
			gl.activeTexture(gl.TEXTURE1);
			gl.bindTexture(gl.TEXTURE_2D, curveTex);
			gl.activeTexture(gl.TEXTURE2);
			gl.bindTexture(gl.TEXTURE_3D, lutTex);

			// Curves: rebake only when inputs change.
			const key = JSON.stringify([
				p.ToneCurvePV2012, p.ToneCurvePV2012Red, p.ToneCurvePV2012Green, p.ToneCurvePV2012Blue,
				p.ParametricShadows, p.ParametricDarks, p.ParametricLights, p.ParametricHighlights,
				p.ParametricShadowSplit, p.ParametricMidtoneSplit, p.ParametricHighlightSplit
			]);
			if (key !== curveKey) {
				curveKey = key;
				curveActive = bakeCurves(p, curveBuf);
				if (curveActive) {
					gl.activeTexture(gl.TEXTURE1);
					gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA8, 256, 1, 0, gl.RGBA, gl.UNSIGNED_BYTE, curveBuf);
				}
			}
			b1('uUseCurve', curveActive);

			gl.uniform2f(loc('uImgSize'), imgW, imgH);
			gl.uniform2f(loc('uRes'), canvas.width, canvas.height);

			// White balance: mired delta -> gain on R/B, tint on G. Luminance-normalised.
			const t = num(p.TemperatureShift) * 0.0045;
			const tn = num(p.TintShift) / 150;
			let wr = Math.exp(t), wg = Math.exp(-tn * 0.35), wb = Math.exp(-t);
			const norm = 0.2126 * wr + 0.7152 * wg + 0.0722 * wb;
			wr /= norm; wg /= norm; wb /= norm;
			f3('uWB', wr, wg, wb);
			f1('uExposure', Math.pow(2, num(p.Exposure2012)));

			f1('uContrast', num(p.Contrast2012) / 100);
			f1('uHighlights', num(p.Highlights2012) / 100);
			f1('uShadows', num(p.Shadows2012) / 100);
			f1('uWhites', num(p.Whites2012) / 100);
			f1('uBlacks', num(p.Blacks2012) / 100);
			f1('uTexture', num(p.Texture) / 100);
			f1('uClarity', num(p.Clarity2012) / 100);
			f1('uDehaze', num(p.Dehaze) / 100);
			const maxDim = Math.max(imgW, imgH);
			f1('uLodTex', Math.max(0, Math.log2(maxDim * 0.004)));
			f1('uLodClar', Math.max(1, Math.log2(maxDim * 0.025)));
			f1('uVibrance', num(p.Vibrance) / 100);
			f1('uSaturation', num(p.Saturation) / 100);

			// HSL
			const hue = new Float32Array(8), sat = new Float32Array(8), lum = new Float32Array(8);
			let useHSL = false;
			HSL_BANDS.forEach((b, i) => {
				hue[i] = num(p[`HueAdjustment${b}`]) / 100;
				sat[i] = num(p[`SaturationAdjustment${b}`]) / 100;
				lum[i] = num(p[`LuminanceAdjustment${b}`]) / 100;
				if (hue[i] || sat[i] || lum[i]) useHSL = true;
			});
			b1('uUseHSL', useHSL);
			gl.uniform1fv(loc('uHue'), hue);
			gl.uniform1fv(loc('uSat'), sat);
			gl.uniform1fv(loc('uLum'), lum);

			// Colour grading
			const g = (h: N, s: N, l: N): [number, number, number] => [num(h), num(s) / 100, num(l) / 100];
			const gs = g(p.SplitToningShadowHue, p.SplitToningShadowSaturation, p.ColorGradeShadowLum);
			const gm = g(p.ColorGradeMidtoneHue, p.ColorGradeMidtoneSat, p.ColorGradeMidtoneLum);
			const gh = g(p.SplitToningHighlightHue, p.SplitToningHighlightSaturation, p.ColorGradeHighlightLum);
			const gg = g(p.ColorGradeGlobalHue, p.ColorGradeGlobalSat, p.ColorGradeGlobalLum);
			f3('uGShadow', ...gs);
			f3('uGMid', ...gm);
			f3('uGHigh', ...gh);
			f3('uGGlobal', ...gg);
			b1('uUseGrade', [gs, gm, gh, gg].some((x) => x[1] !== 0 || x[2] !== 0));
			f1('uBalance', num(p.SplitToningBalance) / 100);
			f1('uBlend', num(p.ColorGradeBlending, 50) / 100);

			// LUT
			const amt = Math.min(1, Math.max(0, opts.lutAmount ?? 1));
			b1('uUseLut', !!lut);
			f1('uLutAmount', amt);
			if (lut) {
				f1('uLutScale', (lut.size - 1) / lut.size);
				f1('uLutOffset', 0.5 / lut.size);
				const dm = lut.domainMin, dM = lut.domainMax;
				f3('uLutDomMin', dm[0], dm[1], dm[2]);
				f3('uLutDomScale', 1 / (dM[0] - dm[0]), 1 / (dM[1] - dm[1]), 1 / (dM[2] - dm[2]));
			}

			f1('uGrain', num(p.GrainAmount) / 100);
			f1('uGrainSize', 1 + (num(p.GrainSize, 25) / 100) * 3);
			f1('uVigAmt', num(p.PostCropVignetteAmount) / 100);
			f1('uVigMid', num(p.PostCropVignetteMidpoint, 50) / 100);
			b1('uGray', !!p.ConvertToGrayscale);
			f1('uSplit', opts.split === undefined ? -1 : opts.split);

			gl.drawArrays(gl.TRIANGLES, 0, 3);
		},

		destroy() {
			destroyed = true;
			gl.deleteTexture(imgTex);
			gl.deleteTexture(curveTex);
			gl.deleteTexture(lutTex);
			gl.deleteBuffer(vbo);
			gl.deleteVertexArray(vao);
			gl.deleteProgram(prog);
			gl.deleteShader(vs);
			gl.deleteShader(fs);
		}
	};
}

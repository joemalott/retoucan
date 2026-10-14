import type { DevelopParams } from './render/renderer';
import type { Prediction } from './api';

// Mirrors presets::BLENDABLE in the Rust core: settings Lightroom scales by
// a preset's Amount. Everything else in a look applies as-is.
const BLENDABLE = new Set([
	'Exposure2012', 'Contrast2012', 'Highlights2012', 'Shadows2012', 'Whites2012', 'Blacks2012',
	'Texture', 'Clarity2012', 'Dehaze', 'Vibrance', 'Saturation',
	'ParametricShadows', 'ParametricDarks', 'ParametricLights', 'ParametricHighlights',
	'SplitToningShadowSaturation', 'SplitToningHighlightSaturation',
	'ColorGradeMidtoneSat', 'ColorGradeGlobalSat', 'ColorGradeShadowLum', 'ColorGradeMidtoneLum',
	'ColorGradeHighlightLum', 'ColorGradeGlobalLum', 'GrainAmount', 'PostCropVignetteAmount'
]);
for (const kind of ['Hue', 'Saturation', 'Luminance'])
	for (const band of ['Red', 'Orange', 'Yellow', 'Green', 'Aqua', 'Blue', 'Purple', 'Magenta'])
		BLENDABLE.add(`${kind}Adjustment${band}`);

const PASS_THROUGH = [
	'ToneCurvePV2012', 'ToneCurvePV2012Red', 'ToneCurvePV2012Green', 'ToneCurvePV2012Blue',
	'SplitToningShadowHue', 'SplitToningHighlightHue', 'SplitToningBalance',
	'ColorGradeMidtoneHue', 'ColorGradeGlobalHue', 'ColorGradeBlending',
	'ParametricShadowSplit', 'ParametricMidtoneSplit', 'ParametricHighlightSplit',
	'GrainSize', 'PostCropVignetteMidpoint', 'ConvertToGrayscale'
];

/** Preview parameters for a photo: the style's correction plus a look at `amount`. */
export function previewParams(
	prediction: Prediction | null,
	look: Record<string, unknown> | null,
	amount: number
): DevelopParams {
	const out: Record<string, unknown> = {};
	if (prediction) {
		for (const [k, v] of Object.entries(prediction.correction)) {
			if (typeof v === 'number' && k !== 'Temperature' && k !== 'Tint') out[k] = v;
		}
		out.TemperatureShift = prediction.temperature_shift;
		out.TintShift = prediction.tint_shift;
	}
	if (look) {
		for (const [k, v] of Object.entries(look)) {
			if (BLENDABLE.has(k) && typeof v === 'number') {
				const base = typeof out[k] === 'number' ? (out[k] as number) : 0;
				out[k] = base + amount * (v - base);
			} else if (PASS_THROUGH.includes(k)) {
				out[k] = v;
			}
		}
	}
	return out as DevelopParams;
}

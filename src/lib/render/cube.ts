/** Parser for Adobe/Resolve .cube 3D LUT files. */

export interface CubeLut {
	title: string;
	size: number;
	/** RGB triplets, red index varies fastest, then green, then blue. Length = size^3 * 3. */
	data: Float32Array;
	/** Input domain (default 0..1). The renderer remaps input into this range. */
	domainMin: [number, number, number];
	domainMax: [number, number, number];
}

export function parseCube(text: string): CubeLut {
	let title = '';
	let size = 0;
	let is1D = false;
	let domainMin: [number, number, number] = [0, 0, 0];
	let domainMax: [number, number, number] = [1, 1, 1];
	let data: Float32Array | null = null;
	let n = 0;
	let expected = 0;

	const lines = text.split(/\r?\n/);
	for (let li = 0; li < lines.length; li++) {
		let line = lines[li];
		const hash = line.indexOf('#');
		if (hash >= 0) line = line.slice(0, hash);
		line = line.trim();
		if (!line) continue;
		const first = line.charCodeAt(0);
		const isNumeric = (first >= 48 && first <= 57) || first === 45 || first === 43 || first === 46;

		if (!isNumeric) {
			const sp = line.search(/\s/);
			const key = (sp < 0 ? line : line.slice(0, sp)).toUpperCase();
			const rest = sp < 0 ? '' : line.slice(sp).trim();
			switch (key) {
				case 'TITLE':
					title = rest.replace(/^"(.*)"$/, '$1');
					break;
				case 'LUT_1D_SIZE':
					is1D = true;
					break;
				case 'LUT_3D_SIZE':
					size = parseInt(rest, 10);
					if (!Number.isFinite(size) || size < 2 || size > 256) {
						throw new Error(`Invalid LUT_3D_SIZE "${rest}" (line ${li + 1})`);
					}
					expected = size * size * size * 3;
					data = new Float32Array(expected);
					break;
				case 'DOMAIN_MIN':
				case 'DOMAIN_MAX': {
					const v = rest.split(/\s+/).map(Number);
					if (v.length !== 3 || v.some((x) => !Number.isFinite(x))) {
						throw new Error(`Invalid ${key} (line ${li + 1})`);
					}
					if (key === 'DOMAIN_MIN') domainMin = [v[0], v[1], v[2]];
					else domainMax = [v[0], v[1], v[2]];
					break;
				}
				default:
					// Unknown keywords (e.g. LUT_IN_VIDEO_RANGE) are ignored.
					break;
			}
			continue;
		}

		if (is1D) throw new Error('1D LUTs (LUT_1D_SIZE) are not supported; a 3D LUT (LUT_3D_SIZE) is required.');
		if (!data) throw new Error(`Data row before LUT_3D_SIZE (line ${li + 1})`);
		const parts = line.split(/\s+/);
		if (parts.length < 3) throw new Error(`Malformed data row (line ${li + 1})`);
		if (n + 3 > expected) throw new Error(`Too many data rows for LUT_3D_SIZE ${size}`);
		for (let k = 0; k < 3; k++) {
			const v = Number(parts[k]);
			if (!Number.isFinite(v)) throw new Error(`Bad number "${parts[k]}" (line ${li + 1})`);
			data[n++] = v;
		}
	}

	if (is1D) throw new Error('1D LUTs (LUT_1D_SIZE) are not supported; a 3D LUT (LUT_3D_SIZE) is required.');
	if (!data || !size) throw new Error('No LUT_3D_SIZE found; not a 3D .cube file.');
	if (n !== expected) throw new Error(`Expected ${expected / 3} data rows, found ${n / 3}.`);
	return { title, size, data, domainMin, domainMax };
}

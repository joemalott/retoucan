import { invoke } from '@tauri-apps/api/core';

export type LookChoice = { uuid: string; name: string; path: string; amount: number };
export type Config = { style: string | null; look: LookChoice | null; retouch: string[] };
export type StyleSummary = {
	id: string;
	name: string;
	photo_count: number;
	source_catalog: string;
	created: string;
	accuracy: [string, number, number][];
	retouch_learned: string[];
};
export type LookSummary = {
	uuid: string;
	name: string;
	group: string;
	kind: string;
	path: string;
	has_table: boolean;
};
export type LightroomState = {
	connected: boolean;
	catalog: string | null;
	selected: string[];
	seconds_ago: number | null;
	plugin_installed: boolean;
};
export type JobStatus = {
	id: string;
	state: 'waiting' | 'done' | 'missing';
	applied: number | null;
	skipped: number | null;
	message: string | null;
	/** While Lightroom works: "Applying edits" or "Detecting masks". */
	stage: string | null;
	done: number | null;
	total: number | null;
};
/** Payload of the `shortcut-edit` event: a serialized Rust Result. */
export type ShortcutResult = { Ok?: JobStatus; Err?: string };
export type Preview = { url: string; width: number; height: number; camera: string };
export type Prediction = {
	correction: Record<string, number | string>;
	temperature_shift: number;
	tint_shift: number;
};
export type CatalogPhoto = {
	path: string;
	name: string;
	folder: string;
	pick: number;
	rating: number;
	label: string | null;
	capture_time: string | null;
	exists: boolean;
	retoucan: boolean;
};
export type RetouchOption = { id: string; label: string };
export type Progress = { stage: string; done: number; total: number };

export const api = {
	getConfig: () => invoke<Config>('get_config'),
	saveConfig: (config: Config) => invoke<void>('save_config', { config }),
	listStyles: () => invoke<StyleSummary[]>('list_styles'),
	deleteStyle: (id: string) => invoke<void>('delete_style', { id }),
	trainStyle: (catalog: string, name: string) => invoke<StyleSummary>('train_style', { catalog, name }),
	listLooks: () => invoke<LookSummary[]>('list_looks'),
	lookParams: (path: string) => invoke<Record<string, unknown>>('look_params', { path }),
	lightroomState: () => invoke<LightroomState>('lightroom_state'),
	installPlugin: () => invoke<string>('install_plugin'),
	samplePhotos: (style: string | null, count: number) => invoke<string[]>('sample_photos', { style, count }),
	loadPreview: (path: string, maxEdge: number) => invoke<Preview>('load_preview', { path, maxEdge }),
	predictOne: (path: string, style: string | null) => invoke<Prediction>('predict_one', { path, style }),
	editSelection: () => invoke<JobStatus>('edit_selection'),
	jobStatus: (id: string) => invoke<JobStatus>('job_status', { id }),
	retouchOptions: () => invoke<RetouchOption[]>('retouch_options'),
	mcpCommand: () => invoke<string>('mcp_command'),
	catalogPhotos: (catalog: string | null) =>
		invoke<{ catalog: string; photos: CatalogPhoto[] }>('catalog_photos', { catalog }),
	thumbnail: (path: string) => invoke<string>('thumbnail', { path }),
	editPhotos: (paths: string[], collection: string | null = null) =>
		invoke<JobStatus>('edit_photos', { paths, collection }),
	photoAction: (action: 'undo' | 'cleanup' | 'check', paths: string[]) => invoke<JobStatus>('photo_action', { action, paths })
};

export function fileName(path: string) {
	return path.split(/[\\/]/).pop() ?? path;
}

/**
 * Evenly spaced picks across a list (e.g. a wedding in capture order), so a
 * preview covers the whole day as the light changes. About one per hundred
 * photos, between `min` and `max`.
 */
export function spread<T>(items: T[], min = 6, max = 12): T[] {
	const n = Math.min(items.length, Math.max(min, Math.min(max, Math.round(items.length / 100) + 1)));
	if (n <= 1) return items.slice(0, n);
	return Array.from({ length: n }, (_, i) => items[Math.round((i * (items.length - 1)) / (n - 1))]);
}

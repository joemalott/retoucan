<script lang="ts">
	import { SvelteSet } from 'svelte/reactivity';
	import { api, fileName, type CatalogPhoto, type LightroomState } from '$lib/api';

	type Tab = 'photos' | 'edit' | 'looks' | 'retouch' | 'styles' | 'connect';
	let {
		lr,
		chosen = $bindable(),
		go
	}: { lr: LightroomState; chosen: string[]; go: (t: Tab) => void } = $props();

	const FLAGS = [
		{ id: 1, label: 'Picks' },
		{ id: 0, label: 'Unflagged' },
		{ id: -1, label: 'Rejected' }
	];
	const LABEL_COLORS: Record<string, string> = {
		Red: '#e5484d',
		Yellow: '#f5c542',
		Green: '#46a758',
		Blue: '#3e8ee6',
		Purple: '#8e4ec6'
	};

	let catalog = $state<string | null>(null);
	let photos = $state<CatalogPhoto[]>([]);
	let loading = $state(false);
	let error = $state<string | null>(null);

	let flags = new SvelteSet<number>();
	let minRating = $state(0);
	let labels = new SvelteSet<string>();
	let folders = new SvelteSet<string>();
	let skipEdited = $state(true);
	let excluded = new SvelteSet<string>();

	let allFolders = $derived([...new Set(photos.map((p) => p.folder))].sort());
	let allLabels = $derived([...new Set(photos.map((p) => p.label).filter((l): l is string => !!l))].sort());
	let counts = $derived({
		1: photos.filter((p) => p.pick === 1).length,
		0: photos.filter((p) => p.pick === 0).length,
		[-1]: photos.filter((p) => p.pick === -1).length
	} as Record<number, number>);
	let edited = $derived(photos.filter((p) => p.retoucan).length);

	let matches = $derived(
		photos.filter(
			(p) =>
				p.exists &&
				flags.has(p.pick) &&
				p.rating >= minRating &&
				(labels.size === 0 || (p.label !== null && labels.has(p.label))) &&
				(folders.size === 0 || folders.has(p.folder)) &&
				!(skipEdited && p.retoucan)
		)
	);

	$effect(() => {
		chosen = matches.filter((p) => !excluded.has(p.path)).map((p) => p.path);
	});

	async function load() {
		loading = true;
		error = null;
		try {
			const r = await api.catalogPhotos(null);
			const first = catalog !== r.catalog;
			catalog = r.catalog;
			photos = r.photos;
			if (first) {
				// Start from the cull: picks if there are any, otherwise everything not rejected.
				flags.clear();
				if (photos.some((p) => p.pick === 1)) flags.add(1);
				else (flags.add(1), flags.add(0));
				labels.clear();
				folders.clear();
				excluded.clear();
				minRating = 0;
			}
		} catch (e) {
			error = String(e);
		} finally {
			loading = false;
		}
	}

	// Load when Lightroom reports a catalog, and again if it switches catalogs.
	$effect(() => {
		if (lr.catalog && lr.catalog !== catalog && !loading) load();
	});

	function toggle<T>(set: SvelteSet<T>, v: T) {
		if (set.has(v)) set.delete(v);
		else set.add(v);
	}

	// Thumbnails load only once their tile scrolls into view.
	const thumbs = $state<Record<string, string>>({});
	function lazy(node: HTMLElement, path: string) {
		const io = new IntersectionObserver(
			(entries) => {
				if (!entries[0].isIntersecting || thumbs[path]) return;
				io.disconnect();
				api.thumbnail(path).then((url) => (thumbs[path] = url)).catch(() => {});
			},
			{ rootMargin: '400px' }
		);
		io.observe(node);
		return { destroy: () => io.disconnect() };
	}
</script>

<div class="page">
	<header>
		<div>
			<h2>Choose photos</h2>
			<p class="muted">
				{#if catalog}
					{fileName(catalog)} · {photos.length} photos. Cull in Lightroom with flags, stars or color labels, then pick which
					ones Retoucan edits.
				{:else}
					Open your catalog in Lightroom Classic. Retoucan reads your flags, stars and color labels from it.
				{/if}
			</p>
		</div>
		<button class="btn ghost small" onclick={load} disabled={loading || !lr.catalog}>
			{loading ? 'Reading…' : 'Refresh'}
		</button>
	</header>

	{#if error}<div class="card err">{error}</div>{/if}

	{#if photos.length}
		<div class="card filters">
			<div class="group">
				<span class="label">Flag</span>
				{#each FLAGS as f}
					<button class="chip" class:on={flags.has(f.id)} onclick={() => toggle(flags, f.id)}>
						{f.label} <span class="n">{counts[f.id]}</span>
					</button>
				{/each}
			</div>
			<div class="group">
				<span class="label">Stars</span>
				{#each [0, 1, 2, 3, 4, 5] as r}
					<button class="chip" class:on={minRating === r} onclick={() => (minRating = r)}>
						{r === 0 ? 'Any' : `${'★'.repeat(r)}+`}
					</button>
				{/each}
			</div>
			{#if allLabels.length}
				<div class="group">
					<span class="label">Label</span>
					{#each allLabels as l}
						<button class="chip" class:on={labels.has(l)} onclick={() => toggle(labels, l)}>
							<span class="swatch" style:background={LABEL_COLORS[l] ?? 'var(--muted)'}></span>{l}
						</button>
					{/each}
				</div>
			{/if}
			{#if allFolders.length > 1}
				<div class="group">
					<span class="label">Folder</span>
					{#each allFolders as f}
						<button class="chip" class:on={folders.has(f)} onclick={() => toggle(folders, f)}>
							{f.replace(/\/$/, '') || 'Top level'}
						</button>
					{/each}
				</div>
			{/if}
			{#if edited}
				<label class="group check">
					<input type="checkbox" bind:checked={skipEdited} />
					<span>Skip {edited} photo{edited === 1 ? '' : 's'} Retoucan already edited</span>
				</label>
			{/if}
		</div>

		<div class="grid">
			{#each matches as p (p.path)}
				{@const off = excluded.has(p.path)}
				<button
					class="tile"
					class:off
					use:lazy={p.path}
					onclick={() => toggle(excluded, p.path)}
					title={off ? 'Left out. Click to include.' : 'Click to leave this photo out.'}
				>
					{#if thumbs[p.path]}<img src={thumbs[p.path]} alt={p.name} />{/if}
					<span class="meta">
						{#if p.label}<span class="swatch" style:background={LABEL_COLORS[p.label] ?? 'var(--muted)'}></span>{/if}
						{#if p.rating}<span>{'★'.repeat(p.rating)}</span>{/if}
						<span class="name">{p.name}</span>
					</span>
				</button>
			{:else}
				<p class="muted none">No photos match these filters.</p>
			{/each}
		</div>
	{/if}

	<footer class="card bar">
		<div>
			<strong>{chosen.length} photo{chosen.length === 1 ? '' : 's'} chosen</strong>
			{#if excluded.size}<span class="muted small"> · {excluded.size} left out by hand</span>{/if}
		</div>
		<button class="btn primary" disabled={!chosen.length} onclick={() => go('looks')}>Next: Look →</button>
	</footer>
</div>

<style>
	.page {
		padding: 28px 32px 20px;
		display: flex;
		flex-direction: column;
		gap: 16px;
		height: 100%;
	}
	header {
		display: flex;
		justify-content: space-between;
		align-items: flex-start;
		gap: 16px;
	}
	header h2 {
		font-size: 26px;
		margin-bottom: 4px;
	}
	.err {
		border-color: var(--rose);
		color: var(--rose);
	}
	.filters {
		display: flex;
		flex-direction: column;
		gap: 10px;
		padding: 14px 16px;
	}
	.group {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 6px;
	}
	.label {
		width: 52px;
		font-size: 12px;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: var(--muted);
	}
	.chip {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		padding: 4px 11px;
		border-radius: 99px;
		font-size: 13px;
		border: 1px solid var(--line);
		background: var(--card);
	}
	.chip.on {
		border-color: var(--beak);
		background: #3a2a18;
	}
	.chip .n {
		color: var(--muted);
		font-size: 12px;
	}
	.swatch {
		width: 9px;
		height: 9px;
		border-radius: 50%;
		display: inline-block;
	}
	.check {
		font-size: 13px;
		gap: 8px;
		cursor: pointer;
	}
	.check input {
		accent-color: var(--beak);
	}
	.grid {
		flex: 1;
		min-height: 0;
		overflow-y: auto;
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
		grid-auto-rows: 150px;
		gap: 8px;
		align-content: start;
	}
	.tile {
		position: relative;
		border-radius: 8px;
		overflow: hidden;
		background: var(--well);
		border: 2px solid transparent;
		padding: 0;
	}
	.tile:hover {
		border-color: var(--line);
	}
	.tile img {
		width: 100%;
		height: 100%;
		object-fit: contain;
	}
	.tile.off img {
		opacity: 0.25;
	}
	.tile.off {
		border-color: var(--rose);
		border-style: dashed;
	}
	.meta {
		position: absolute;
		left: 0;
		right: 0;
		bottom: 0;
		display: flex;
		align-items: center;
		gap: 6px;
		padding: 3px 6px;
		font-size: 11px;
		background: linear-gradient(transparent, rgba(0, 0, 0, 0.7));
		color: #f3efe8;
	}
	.meta .name {
		margin-left: auto;
		color: #cfc9bf;
	}
	.none {
		grid-column: 1 / -1;
	}
	.bar {
		display: flex;
		justify-content: space-between;
		align-items: center;
		padding: 12px 16px;
	}
</style>

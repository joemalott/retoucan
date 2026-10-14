<script lang="ts">
	import { onDestroy, untrack } from 'svelte';
	import SampleView from './SampleView.svelte';
	import {
		api,
		fileName,
		spread,
		type Config,
		type JobStatus,
		type ShortcutResult,
		type LightroomState,
		type StyleSummary
	} from '$lib/api';

	type Tab = 'photos' | 'edit' | 'looks' | 'retouch' | 'styles' | 'connect';
	let {
		config,
		styles,
		lr,
		chosen = [],
		go,
		shortcut = null
	}: {
		config: Config;
		styles: StyleSummary[];
		lr: LightroomState;
		/** Photos chosen on the Photos screen; when empty, edit Lightroom's selection. */
		chosen?: string[];
		go: (t: Tab) => void;
		shortcut?: ShortcutResult | null;
	} = $props();

	const TEST_COLLECTION = 'Retoucan test batch';
	const RETOUCH_LABELS: Record<string, string> = {
		subject: 'Brighten subject',
		background: 'Darken background',
		skin: 'Smooth skin',
		teeth: 'Whiten teeth',
		eyes: 'Brighten eyes',
		sky: 'Deepen sky'
	};

	let targets = $derived(chosen.length ? chosen : lr.selected);
	let style = $derived(styles.find((s) => s.id === config.style) ?? styles[0]);
	let lookParams = $state<Record<string, unknown> | null>(null);
	$effect(() => {
		const look = config.look;
		lookParams = null;
		if (look) api.lookParams(look.path).then((p) => (lookParams = p)).catch(() => {});
	});

	// Preview a spread of the set (the Photos screen lists it in capture order),
	// so getting ready, ceremony, portraits and reception all show up. The same
	// photos make the test batch.
	let samples = $derived(spread(targets));
	let picked = $state(0);
	let bigPath = $derived(samples[Math.min(picked, samples.length - 1)] ?? null);

	let setup = $derived([
		{ done: styles.length > 0, label: 'Learn your style from an edited catalog', tab: 'styles' as Tab },
		{ done: lr.plugin_installed, label: 'Install the Lightroom plugin', tab: 'connect' as Tab },
		{ done: lr.connected, label: 'Open Lightroom Classic', tab: 'connect' as Tab }
	]);

	// The flow: try the samples, then edit the rest, then keep or undo.
	type Phase = 'ready' | 'testing' | 'tested' | 'editing' | 'edited' | 'undoing' | 'cleaning' | 'checking' | 'finished';
	let phase = $state<Phase>('ready');
	let tested = $state<string[]>([]);
	let edited = $state<string[]>([]);
	let job = $state<JobStatus | null>(null);
	let error = $state<string | null>(null);
	let notice = $state<string | null>(null);
	let busy = $derived(['testing', 'editing', 'undoing', 'cleaning', 'checking'].includes(phase));
	let rest = $derived(targets.filter((p) => !tested.includes(p)));

	// A new set of photos starts the flow over.
	let lastTargets = '';
	$effect(() => {
		const key = targets.join('\n');
		untrack(() => {
			if (key === lastTargets || busy) return;
			lastTargets = key;
			phase = 'ready';
			tested = [];
			edited = [];
			job = null;
			notice = null;
		});
	});

	// Lightroom can take a long time on big jobs (mask detection is a few
	// seconds a photo), so keep following the job until it's done.
	let alive = true;
	onDestroy(() => (alive = false));
	type StageStart = { stage: string; done: number; at: number };
	let stageStart = $state<StageStart | null>(null);
	let eta = $derived.by(() => {
		if (!job?.stage || job.done == null || !job.total || !stageStart || stageStart.stage !== job.stage) return null;
		const did = job.done - stageStart.done;
		if (did < 2) return null;
		const perPhoto = (Date.now() - stageStart.at) / did;
		return Math.max(1, Math.round((perPhoto * (job.total - job.done)) / 60000));
	});

	async function follow(start: Promise<JobStatus>): Promise<JobStatus> {
		stageStart = null;
		job = await start;
		while (alive && job.state === 'waiting') {
			await new Promise((r) => setTimeout(r, 1500));
			job = await api.jobStatus(job.id);
			// (Cast: TS narrows stageStart to null after the reset above.)
			if (job.stage && (stageStart as StageStart | null)?.stage !== job.stage) {
				stageStart = { stage: job.stage, done: job.done ?? 0, at: Date.now() };
			}
		}
		return job;
	}

	async function run(during: Phase, after: Phase, start: () => Promise<JobStatus>, onDone?: () => void) {
		error = null;
		notice = null;
		const before = phase;
		phase = during;
		try {
			await follow(start());
			onDone?.();
			phase = after;
		} catch (e) {
			error = String(e);
			phase = before;
		}
	}

	const tryTest = () =>
		run('testing', 'tested', () => api.editPhotos(samples, TEST_COLLECTION), () => {
			tested = [...samples];
			edited = [...samples];
		});
	const editRest = () =>
		run('editing', 'edited', () => api.editPhotos(rest), () => {
			edited = [...new Set([...edited, ...rest])];
		});
	const editAll = () =>
		run('editing', 'edited', () => (chosen.length ? api.editPhotos(targets) : api.editSelection()), () => {
			edited = [...targets];
		});
	const undoTest = () =>
		run('undoing', 'ready', () => api.photoAction('undo', tested), () => {
			edited = edited.filter((p) => !tested.includes(p));
			tested = [];
			notice = 'The samples are back to how they were before Retoucan.';
		});
	const undoAll = () =>
		run('undoing', 'ready', () => api.photoAction('undo', edited), () => {
			notice = `${edited.length} photos are back to how they were before Retoucan.`;
			edited = [];
			tested = [];
		});
	// Re-run Lightroom's mask detection and drop masks that found nothing.
	const checkMasks = () => run('checking', 'edited', () => api.photoAction('check', edited));
	const keep = () =>
		run('cleaning', 'finished', () => api.photoAction('cleanup', edited));

	// A job started by the global shortcut (⌃⌥⌘E / Ctrl+Alt+Shift+E) edits
	// Lightroom's selection directly; follow it like Edit all.
	$effect(() => {
		const r = shortcut;
		if (!r) return;
		// Only the shortcut event should trigger this, not the phase changes run() makes.
		untrack(() => {
			const paths = [...lr.selected];
			run('editing', 'edited', () => (r.Ok ? Promise.resolve(r.Ok) : Promise.reject(r.Err)), () => {
				edited = paths;
			});
		});
	});

	let plural = (n: number) => `${n} photo${n === 1 ? '' : 's'}`;
	let blocked = $derived(!lr.connected ? 'Open Lightroom Classic to edit.' : !style ? 'Learn a style first.' : null);
</script>

<div class="page">
	<header>
		<h2>Review &amp; send</h2>
		<p class="muted">
			Check the recipe and the preview, try it on a few photos, then edit the rest. Every change stays editable in
			Lightroom, and you can undo it.
		</p>
	</header>

	{#if setup.some((s) => !s.done)}
		<div class="card setup">
			<h3>Before you start</h3>
			<ol>
				{#each setup as s}
					<li class:done={s.done}>
						<span class="check">{s.done ? '✓' : ''}</span>
						<span>{s.label}</span>
						{#if !s.done}<button class="btn ghost small" onclick={() => go(s.tab)}>Set up</button>{/if}
					</li>
				{/each}
			</ol>
		</div>
	{/if}

	<div class="grid">
		<div class="card recipe">
			<button class="row" onclick={() => go('photos')}>
				<span class="label">Photos</span>
				<span class="value">
					{targets.length ? plural(targets.length) : 'None chosen'}
					{#if !chosen.length && targets.length}<span class="muted small"> · Lightroom selection</span>{/if}
				</span>
				<span class="edit">Change</span>
			</button>
			<button class="row" onclick={() => go('styles')}>
				<span class="label">Style</span>
				<span class="value">{style?.name ?? 'None yet'}</span>
				<span class="edit">Change</span>
			</button>
			<button class="row" onclick={() => go('looks')}>
				<span class="label">Look</span>
				<span class="value">
					{config.look ? `${config.look.name.trim()} · ${Math.round(config.look.amount * 100)}%` : 'None'}
				</span>
				<span class="edit">Change</span>
			</button>
			<button class="row" onclick={() => go('retouch')}>
				<span class="label">Retouch</span>
				<span class="value">
					{config.retouch.length ? config.retouch.map((r) => RETOUCH_LABELS[r] ?? r).join(', ') : 'None'}
				</span>
				<span class="edit">Change</span>
			</button>

			<div class="actions">
				{#if blocked}
					<p class="muted small center">{blocked}</p>
				{:else if !targets.length}
					<button class="btn primary big" onclick={() => go('photos')}>Choose photos</button>
				{:else if busy && job}
					<div class="job">
						{#if job.state === 'waiting' && job.stage && job.total}
							<div class="prog-head">
								<span>{job.stage}</span>
								<span class="muted">{job.done} of {job.total}{eta != null ? ` · about ${eta} min left` : ''}</span>
							</div>
							<div class="bar"><div style:width="{((job.done ?? 0) / job.total) * 100}%"></div></div>
							<div class="muted small">You can keep working in Lightroom while this runs.</div>
						{:else}
							<div class="muted">
								{lr.connected ? 'Handing the photos to Lightroom…' : 'Queued. Lightroom applies it when you open it.'}
							</div>
						{/if}
					</div>
				{:else if busy}
					<div class="job muted">Getting the edits ready…</div>
				{:else if phase === 'ready'}
					{#if targets.length > samples.length}
						<button class="btn primary big" onclick={tryTest}>Try on {plural(samples.length)} first</button>
						<p class="muted small center">
							Edits the samples on the right and puts them in a “{TEST_COLLECTION}” collection in Lightroom, so you can
							check the real result before doing all {targets.length}.
						</p>
						<button class="link center" onclick={editAll}>Skip the test and edit all {targets.length}</button>
					{:else}
						<button class="btn primary big" onclick={editAll}>Edit {plural(targets.length)}</button>
					{/if}
				{:else if phase === 'tested'}
					<div class="job ok">
						<strong>{plural(tested.length)} edited.</strong>
						<div class="muted small">
							Lightroom is showing them in the “{TEST_COLLECTION}” collection. Happy with them? Edit the rest. Not quite?
							Undo, adjust the look or retouching, and try again.
						</div>
					</div>
					<button class="btn primary big" onclick={editRest}>Looks good, edit the other {rest.length}</button>
					<button class="btn" onclick={undoTest}>Undo the samples</button>
				{:else if phase === 'edited'}
					<div class="job ok">
						<strong>Done. {plural(edited.length)} edited.</strong>
						{#if job?.message}<div class="muted small">{job.message}</div>{/if}
						<div class="muted small">
							Each photo has a “Before Retoucan” snapshot, so you can undo. When you're happy, keep the edits and clean
							the snapshots up.
						</div>
					</div>
					<button class="btn primary big" onclick={keep}>Keep edits &amp; clean up</button>
					<button class="btn" onclick={checkMasks}>Check masks again</button>
					<button class="btn" onclick={undoAll}>Undo all {edited.length}</button>
				{:else if phase === 'finished'}
					<div class="job ok"><strong>All set.</strong> The edits are yours to fine-tune in Lightroom.</div>
				{/if}
				{#if notice}<div class="job">{notice}</div>{/if}
				{#if error}<div class="job err">{error}</div>{/if}
			</div>
		</div>

		<div class="card preview">
			{#if bigPath && style}
				<div class="pv">
					<SampleView
						path={bigPath}
						styleId={style.id}
						{lookParams}
						amount={config.look?.amount ?? 1}
						edge={1600}
						compare
					/>
				</div>
				{#if samples.length > 1}
					<div class="strip">
						{#each samples as path, i (path)}
							<button
								class="sample"
								class:on={path === bigPath}
								class:done={edited.includes(path)}
								onclick={() => (picked = i)}
								title={fileName(path)}
							>
								<SampleView {path} styleId={style.id} {lookParams} amount={config.look?.amount ?? 1} edge={480} />
							</button>
						{/each}
					</div>
				{/if}
				<p class="muted small cap">
					{fileName(bigPath)}. Drag to compare. {samples.length > 1
						? `${samples.length} samples spread across the set. `
						: ''}Approximate; Lightroom renders the final result and the masks.
				</p>
			{:else}
				<div class="empty muted">
					{lr.connected ? 'Choose photos to preview them here.' : 'A preview appears here when Lightroom is open.'}
				</div>
			{/if}
		</div>
	</div>
</div>

<style>
	.page {
		padding: 28px 32px;
		display: flex;
		flex-direction: column;
		gap: 20px;
		height: 100%;
	}
	header h2 {
		font-size: 26px;
		margin-bottom: 4px;
	}
	.setup ol {
		list-style: none;
		padding: 0;
		margin: 12px 0 0;
		display: flex;
		flex-direction: column;
		gap: 6px;
	}
	.setup li {
		display: flex;
		align-items: center;
		gap: 10px;
	}
	.setup li.done {
		color: var(--muted);
	}
	.check {
		width: 22px;
		height: 22px;
		border-radius: 50%;
		border: 1.5px solid var(--line);
		display: grid;
		place-items: center;
		font-size: 12px;
		color: #1a1208;
	}
	li.done .check {
		background: var(--leaf);
		border-color: var(--leaf);
	}
	.grid {
		display: grid;
		grid-template-columns: 340px 1fr;
		gap: 20px;
		flex: 1;
		min-height: 420px;
	}
	.recipe {
		display: flex;
		flex-direction: column;
		gap: 4px;
		overflow-y: auto;
	}
	.row {
		display: grid;
		grid-template-columns: 1fr auto;
		grid-template-areas: 'label edit' 'value value';
		text-align: left;
		gap: 2px 8px;
		padding: 10px 8px;
		border-radius: 8px;
		border-bottom: 1px solid var(--line);
	}
	.row:hover {
		background: var(--card);
	}
	.row:hover .edit {
		color: var(--beak);
	}
	.label {
		grid-area: label;
		font-size: 12px;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: var(--muted);
	}
	.value {
		grid-area: value;
		font-weight: 550;
	}
	.edit {
		grid-area: edit;
		font-size: 12px;
		color: var(--muted);
	}
	.actions {
		margin-top: auto;
		padding-top: 16px;
		display: flex;
		flex-direction: column;
		gap: 10px;
	}
	.actions .btn {
		justify-content: center;
	}
	.center {
		text-align: center;
	}
	.link {
		color: var(--beak);
		text-decoration: underline;
		padding: 0;
		font-size: 13px;
	}
	.job {
		padding: 12px;
		border-radius: 10px;
		background: var(--card);
		font-size: 14px;
		display: flex;
		flex-direction: column;
		gap: 4px;
	}
	.job.ok {
		border: 1px solid var(--leaf);
	}
	.job.err {
		border: 1px solid var(--rose);
	}
	.prog-head {
		display: flex;
		justify-content: space-between;
		gap: 8px;
	}
	.bar {
		height: 6px;
		border-radius: 99px;
		background: var(--well);
		overflow: hidden;
		margin: 4px 0;
	}
	.bar div {
		height: 100%;
		background: linear-gradient(90deg, var(--beak), var(--beak-2));
		transition: width 0.4s;
	}
	.preview {
		display: flex;
		flex-direction: column;
		gap: 10px;
		min-height: 0;
	}
	.pv {
		flex: 1;
		min-height: 0;
	}
	.strip {
		display: grid;
		grid-auto-flow: column;
		grid-auto-columns: 1fr;
		gap: 8px;
		height: 96px;
	}
	.sample {
		position: relative;
		padding: 0;
		border-radius: 8px;
		border: 2px solid transparent;
		overflow: hidden;
		min-width: 0;
		background: var(--well);
	}
	.sample.on {
		border-color: var(--beak);
	}
	.sample.done::after {
		content: '✓';
		position: absolute;
		top: 4px;
		right: 4px;
		width: 18px;
		height: 18px;
		border-radius: 50%;
		background: var(--leaf);
		color: #10200f;
		font-size: 11px;
		display: grid;
		place-items: center;
	}
	.cap {
		text-align: center;
	}
	.empty {
		flex: 1;
		display: grid;
		place-items: center;
		text-align: center;
	}
	.small {
		font-size: 13px;
	}
</style>

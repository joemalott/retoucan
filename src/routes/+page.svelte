<script lang="ts">
	import { onMount } from 'svelte';
	import Logo from '$lib/components/Logo.svelte';
	import PhotosTab from '$lib/components/PhotosTab.svelte';
	import EditTab from '$lib/components/EditTab.svelte';
	import LooksTab from '$lib/components/LooksTab.svelte';
	import RetouchTab from '$lib/components/RetouchTab.svelte';
	import StylesTab from '$lib/components/StylesTab.svelte';
	import ConnectTab from '$lib/components/ConnectTab.svelte';
	import { listen } from '@tauri-apps/api/event';
	import { api, type Config, type LightroomState, type ShortcutResult, type StyleSummary } from '$lib/api';

	type Tab = 'photos' | 'edit' | 'looks' | 'retouch' | 'styles' | 'connect';
	// The editing flow, in order. Styles and Lightroom are one-time setup.
	const STEPS: Tab[] = ['photos', 'looks', 'retouch', 'edit'];
	const STEP_LABELS: Record<string, string> = { photos: 'Photos', looks: 'Look', retouch: 'Retouch', edit: 'Review & send' };

	let tab = $state<Tab>('photos');
	let chosen = $state<string[]>([]);
	let config = $state<Config>({ style: null, look: null, retouch: [] });
	let styles = $state<StyleSummary[]>([]);
	let lr = $state<LightroomState>({
		connected: false,
		catalog: null,
		selected: [],
		seconds_ago: null,
		plugin_installed: false
	});

	let shortcut = $state<ShortcutResult | null>(null);

	let styleName = $derived((styles.find((s) => s.id === config.style) ?? styles[0])?.name ?? 'None yet');
	let summaries = $derived<Record<string, string>>({
		photos: chosen.length ? `${chosen.length} photos` : 'Choose from your cull',
		looks: config.look ? `${config.look.name.trim()} · ${Math.round(config.look.amount * 100)}%` : 'No look',
		retouch: config.retouch.length ? `${config.retouch.length} retouch${config.retouch.length === 1 ? '' : 'es'}` : 'None',
		edit: 'Try, edit, undo'
	});
	let stepIndex = $derived(STEPS.indexOf(tab));

	function go(t: Tab) {
		tab = t;
		shortcut = null;
	}

	async function refresh() {
		[config, styles] = await Promise.all([api.getConfig(), api.listStyles()]);
	}

	async function save(next: Config) {
		config = next;
		await api.saveConfig(next);
	}

	onMount(() => {
		refresh();
		const poll = async () => (lr = await api.lightroomState());
		poll();
		const t = setInterval(poll, 2000);
		// The global edit shortcut fired: show the Edit tab so its progress or error is visible.
		const off = listen<ShortcutResult>('shortcut-edit', (e) => {
			tab = 'edit';
			shortcut = e.payload;
			poll();
		});
		return () => {
			clearInterval(t);
			off.then((f) => f());
		};
	});
</script>

<div class="app">
	<aside>
		<div class="brand">
			<Logo size={36} />
			<div>
				<h1>Retoucan</h1>
				<p class="muted small">Your style, first pass</p>
			</div>
		</div>
		<nav>
			{#each STEPS as id, i}
				<button class="step" class:active={tab === id} onclick={() => go(id)}>
					<span class="num">{i + 1}</span>
					<span class="text">
						<span>{STEP_LABELS[id]}</span>
						<small>{summaries[id]}</small>
					</span>
				</button>
			{/each}
		</nav>
		<div class="setup">
			<div class="setup-label">Setup</div>
			<button class:active={tab === 'styles'} onclick={() => go('styles')}>
				<span>Styles</span>
				<small>{styleName}</small>
			</button>
			<button class:active={tab === 'connect'} onclick={() => go('connect')}>
				<span class="conn">
					<span class="dot" class:on={lr.connected} class:warn={!lr.connected && lr.plugin_installed}></span>
					Lightroom
				</span>
				<small>
					{lr.connected ? 'Connected' : lr.plugin_installed ? 'Closed' : 'Plugin not installed'}
				</small>
			</button>
		</div>
	</aside>

	<main>
		<!-- Photos and Review stay mounted so filters, exclusions and a running
		     job survive moving between steps. -->
		<div class="keep" class:hidden={tab !== 'photos'}>
			<PhotosTab {lr} bind:chosen {go} />
		</div>
		<div class="keep" class:hidden={tab !== 'edit'}>
			<EditTab {config} {styles} {lr} {chosen} {shortcut} {go} />
		</div>
		{#if tab === 'looks' || tab === 'retouch'}
			<div class="stepwrap">
				<div class="stepbody">
					{#if tab === 'looks'}
						<LooksTab {config} {save} {chosen} {lr} />
					{:else}
						<RetouchTab {config} {styles} {save} />
					{/if}
				</div>
				<div class="stepnav">
					<button class="btn ghost" onclick={() => go(STEPS[stepIndex - 1])}>← {STEP_LABELS[STEPS[stepIndex - 1]]}</button>
					<button class="btn primary" onclick={() => go(STEPS[stepIndex + 1])}>
						Next: {STEP_LABELS[STEPS[stepIndex + 1]]} →
					</button>
				</div>
			</div>
		{:else if tab === 'styles'}
			<StylesTab {config} {styles} {save} {refresh} />
		{:else if tab === 'connect'}
			<ConnectTab {lr} />
		{/if}
	</main>
</div>

<style>
	.keep {
		height: 100%;
	}
	.keep.hidden {
		display: none;
	}
	.app {
		display: grid;
		grid-template-columns: 232px 1fr;
		height: 100vh;
	}
	aside {
		background: var(--panel);
		border-right: 1px solid var(--line);
		display: flex;
		flex-direction: column;
		padding: 20px 14px;
		gap: 24px;
	}
	.brand {
		display: flex;
		gap: 10px;
		align-items: center;
		padding: 0 6px;
	}
	.brand h1 {
		font-size: 19px;
	}
	nav,
	.setup {
		display: flex;
		flex-direction: column;
		gap: 4px;
	}
	nav button,
	.setup button {
		text-align: left;
		padding: 10px 12px;
		border-radius: 10px;
		display: flex;
		flex-direction: column;
		transition: background 0.12s;
	}
	nav button:hover,
	.setup button:hover {
		background: var(--card);
	}
	nav button.active,
	.setup button.active {
		background: var(--card);
		box-shadow: inset 3px 0 0 var(--beak);
	}
	small {
		color: var(--muted);
		font-size: 12px;
	}
	.step {
		flex-direction: row !important;
		align-items: center;
		gap: 12px;
	}
	.num {
		flex: none;
		width: 24px;
		height: 24px;
		border-radius: 50%;
		display: grid;
		place-items: center;
		font-size: 12px;
		font-weight: 650;
		border: 1.5px solid var(--line);
		color: var(--muted);
	}
	.step.active .num {
		border-color: var(--beak);
		background: var(--beak);
		color: #1a1208;
	}
	.text {
		display: flex;
		flex-direction: column;
		min-width: 0;
	}
	.text small {
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	.setup {
		margin-top: auto;
		padding-top: 12px;
		border-top: 1px solid var(--line);
	}
	.setup-label {
		font-size: 11px;
		text-transform: uppercase;
		letter-spacing: 0.08em;
		color: var(--muted);
		padding: 0 12px 4px;
	}
	.conn {
		display: flex;
		align-items: center;
		gap: 8px;
	}
	.stepwrap {
		height: 100%;
		display: flex;
		flex-direction: column;
	}
	.stepbody {
		flex: 1;
		min-height: 0;
		overflow: auto;
	}
	.stepnav {
		display: flex;
		justify-content: space-between;
		padding: 12px 32px 20px;
		border-top: 1px solid var(--line);
		background: var(--bg);
	}
	main {
		overflow: auto;
		min-width: 0;
	}
</style>

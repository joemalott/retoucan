<script lang="ts">
	import { onMount } from 'svelte';
	import Logo from '$lib/components/Logo.svelte';
	import EditTab from '$lib/components/EditTab.svelte';
	import LooksTab from '$lib/components/LooksTab.svelte';
	import RetouchTab from '$lib/components/RetouchTab.svelte';
	import StylesTab from '$lib/components/StylesTab.svelte';
	import ConnectTab from '$lib/components/ConnectTab.svelte';
	import { api, type Config, type LightroomState, type StyleSummary } from '$lib/api';

	type Tab = 'edit' | 'looks' | 'retouch' | 'styles' | 'connect';
	const tabs: { id: Tab; label: string; hint: string }[] = [
		{ id: 'edit', label: 'Edit', hint: 'Send photos to Lightroom' },
		{ id: 'looks', label: 'Looks', hint: 'Presets & preview' },
		{ id: 'retouch', label: 'Retouch', hint: 'Skin, teeth, eyes…' },
		{ id: 'styles', label: 'Styles', hint: 'Learn how you edit' },
		{ id: 'connect', label: 'Connect', hint: 'Lightroom & AI tools' }
	];

	let tab = $state<Tab>('edit');
	let config = $state<Config>({ style: null, look: null, retouch: [] });
	let styles = $state<StyleSummary[]>([]);
	let lr = $state<LightroomState>({
		connected: false,
		catalog: null,
		selected: [],
		seconds_ago: null,
		plugin_installed: false
	});

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
		return () => clearInterval(t);
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
			{#each tabs as t}
				<button class:active={tab === t.id} onclick={() => (tab = t.id)}>
					<span>{t.label}</span>
					<small>{t.hint}</small>
				</button>
			{/each}
		</nav>
		<div class="lr">
			<span class="dot" class:on={lr.connected} class:warn={!lr.connected && lr.plugin_installed}></span>
			<div>
				<div class="small">
					{lr.connected ? 'Lightroom connected' : lr.plugin_installed ? 'Lightroom closed' : 'Plugin not installed'}
				</div>
				{#if lr.connected}
					<div class="muted small">{lr.selected.length} selected</div>
				{/if}
			</div>
		</div>
	</aside>

	<main>
		{#if tab === 'edit'}
			<EditTab {config} {styles} {lr} go={(t: Tab) => (tab = t)} />
		{:else if tab === 'looks'}
			<LooksTab {config} {save} />
		{:else if tab === 'retouch'}
			<RetouchTab {config} {styles} {save} />
		{:else if tab === 'styles'}
			<StylesTab {config} {styles} {save} {refresh} />
		{:else}
			<ConnectTab {lr} />
		{/if}
	</main>
</div>

<style>
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
	nav {
		display: flex;
		flex-direction: column;
		gap: 4px;
	}
	nav button {
		text-align: left;
		padding: 10px 12px;
		border-radius: 10px;
		display: flex;
		flex-direction: column;
		transition: background 0.12s;
	}
	nav button:hover {
		background: var(--card);
	}
	nav button.active {
		background: var(--card);
		box-shadow: inset 3px 0 0 var(--beak);
	}
	nav small {
		color: var(--muted);
		font-size: 12px;
	}
	.lr {
		margin-top: auto;
		display: flex;
		gap: 10px;
		align-items: center;
		padding: 12px;
		border-radius: 10px;
		background: var(--card);
	}
	main {
		overflow: auto;
		min-width: 0;
	}
</style>
